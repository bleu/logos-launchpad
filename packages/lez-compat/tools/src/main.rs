use lee_core::account::AccountId;
use quote::ToTokens;
use serde_json::{Value, json};
use std::error::Error;

fn fields(
    fields: &syn::Fields,
    constants: &std::collections::BTreeMap<String, String>,
) -> Vec<Value> {
    fields.iter().enumerate().map(|(index, field)| {
        let mut field_type = field.ty.to_token_stream().to_string().replace(' ', "");
        for (name, value) in constants {
            field_type = field_type.replace(&format!(";{name}]"), &format!(";{value}]"));
        }
        json!({
            "name": field.ident.as_ref().map(ToString::to_string).unwrap_or_else(|| index.to_string()),
            "type": field_type,
        })
    }).collect()
}

fn encoded(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("derive")
            && attribute
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
                )
                .is_ok_and(|paths| {
                    paths.iter().any(|path| {
                        path.segments
                            .last()
                            .is_some_and(|segment| segment.ident == "BorshSerialize")
                    })
                })
    })
}

fn module(source: &str, include_private: bool) -> Result<Value, Box<dyn Error>> {
    let mut types = serde_json::Map::new();
    let source = syn::parse_file(source)?;
    let constants = source
        .items
        .iter()
        .filter_map(|item| {
            if let syn::Item::Const(item) = item
                && let syn::Expr::Lit(expression) = item.expr.as_ref()
                && let syn::Lit::Int(value) = &expression.lit
            {
                return Some((item.ident.to_string(), value.base10_digits().to_string()));
            }
            None
        })
        .collect();
    for item in source.items {
        match item {
            syn::Item::Enum(item)
                if (include_private || matches!(item.vis, syn::Visibility::Public(_)))
                    && encoded(&item.attrs) =>
            {
                let variants: Vec<Value> = item.variants.iter().enumerate().map(|(tag, variant)| {
                    json!({ "tag": tag, "name": variant.ident.to_string(), "fields": fields(&variant.fields, &constants) })
                }).collect();
                types.insert(item.ident.to_string(), json!({"variants": variants}));
            }
            syn::Item::Struct(item)
                if (include_private || matches!(item.vis, syn::Visibility::Public(_)))
                    && encoded(&item.attrs) =>
            {
                types.insert(
                    item.ident.to_string(),
                    json!({"fields": fields(&item.fields, &constants)}),
                );
            }
            _ => {}
        }
    }
    Ok(Value::Object(types))
}

fn resolve_qualified_types(
    value: &mut Value,
    registered: &std::collections::BTreeSet<String>,
    dependencies: &mut std::collections::BTreeSet<String>,
) {
    match value {
        Value::Object(object) => {
            if let Some(Value::String(field_type)) = object.get_mut("type") {
                let mut resolved = String::new();
                let mut remaining = field_type.as_str();
                while !remaining.is_empty() {
                    let length = remaining
                        .chars()
                        .take_while(|character| character.is_alphanumeric() || *character == '_')
                        .map(char::len_utf8)
                        .sum::<usize>();
                    if length == 0 {
                        let character = remaining.chars().next().unwrap();
                        resolved.push(character);
                        remaining = &remaining[character.len_utf8()..];
                    } else {
                        let name = &remaining[..length];
                        remaining = &remaining[length..];
                        let module = name.strip_suffix("_core").unwrap_or(name);
                        if remaining.starts_with("::") && registered.contains(module) {
                            resolved.push_str(module);
                            dependencies.insert(module.to_owned());
                        } else {
                            resolved.push_str(name);
                        }
                    }
                }
                *field_type = resolved;
            }
            for child in object.values_mut() {
                resolve_qualified_types(child, registered, dependencies);
            }
        }
        Value::Array(values) => {
            for child in values {
                resolve_qualified_types(child, registered, dependencies);
            }
        }
        _ => {}
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command] if command == "idl" => {
            let core = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src");
            let idl = json!({
                "encoding": "borsh", "version": 1,
                "upstream_types": {
                    "AccountId": "[u8;32]", "PdaSeed": "[u8;32]",
                    "Actor": [{"name": "account_id", "type": "AccountId"}, {"name": "program_account_id", "type": "AccountId"}],
                    "StateReply": [{"name": "state", "type": "Vec<u8>"}],
                },
                "modules": {
                    "token": module(&std::fs::read_to_string(core.join("token.rs"))?, false)?,
                    "authority": module(&std::fs::read_to_string(core.join("authority.rs"))?, false)?,
                }
            });
            println!("{}", serde_json::to_string_pretty(&idl)?);
        }
        [command, sources @ ..] if command == "idl-source" && !sources.is_empty() => {
            let mut modules = serde_json::Map::new();
            let mut parsed = std::collections::BTreeMap::new();
            for source in sources {
                let (name, path) = source.split_once('=').ok_or("expected module=source.rs")?;
                let source = std::fs::read_to_string(path)?;
                modules.insert(name.to_string(), module(&source, true)?);
                parsed.insert(name, syn::parse_file(&source)?);
            }
            let mut imports = std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
            for (parent, source) in &parsed {
                for item in &source.items {
                    if let syn::Item::Use(item) = item
                        && let syn::UseTree::Path(path) = &item.tree
                    {
                        let name = path.ident.to_string();
                        let name = name.strip_suffix("_core").unwrap_or(&name);
                        if parsed.contains_key(name) && name != *parent {
                            imports.entry(parent.to_string()).or_default().insert(name.to_string());
                        }
                    }
                    if let syn::Item::Mod(child) = item {
                        let name = child.ident.to_string();
                        if parsed.get(name.as_str()).is_some_and(|source| {
                            source.items.iter().any(|item| {
                                matches!(item, syn::Item::Use(item)
                                    if item.tree.to_token_stream().to_string().replace(' ', "") == "super::*")
                            })
                        }) {
                            imports.entry(name).or_default().insert(parent.to_string());
                        }
                    }
                }
            }
            let registered = parsed.keys().map(|name| (*name).to_owned()).collect();
            for (name, module) in &mut modules {
                let mut dependencies = std::collections::BTreeSet::new();
                resolve_qualified_types(module, &registered, &mut dependencies);
                dependencies.remove(name);
                if !dependencies.is_empty() {
                    imports.entry(name.clone()).or_default().extend(dependencies);
                }
            }
            println!("{}", serde_json::to_string_pretty(&json!({
                "encoding": "borsh", "version": 1, "modules": modules,
                "imports": imports,
                "upstream_types": {
                    "AccountId": "[u8;32]", "PdaSeed": "[u8;32]",
                    "Actor": [{"name": "account_id", "type": "AccountId"}, {"name": "program_account_id", "type": "AccountId"}],
                    "StateReply": [{"name": "state", "type": "Vec<u8>"}],
                }
            }))?);
        }
        [command, definition, recipient, amount] if command == "encode-transfer" => {
            let account = |value: &str| -> Result<AccountId, Box<dyn Error>> {
                Ok(AccountId::new(hex::decode(value)?.try_into().map_err(|_| "account ID must contain 32 bytes")?))
            };
            let message = lez_compat_core::token::Message::Transfer {
                definition: account(definition)?, recipient: account(recipient)?, amount: amount.parse()?,
            };
            println!("{}", hex::encode(borsh::to_vec(&message)?));
        }
        _ => return Err("usage: lez-compat-tools idl | idl-source <module=source.rs>... | encode-transfer <definition hex> <recipient hex> <amount>".into()),
    }
    Ok(())
}
