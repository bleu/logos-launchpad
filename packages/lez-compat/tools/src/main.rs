use lee_core::account::AccountId;
use quote::ToTokens;
use serde_json::{Value, json};
use std::error::Error;

fn fields(fields: &syn::Fields) -> Vec<Value> {
    fields.iter().enumerate().map(|(index, field)| json!({
        "name": field.ident.as_ref().map(ToString::to_string).unwrap_or_else(|| index.to_string()),
        "type": field.ty.to_token_stream().to_string().replace(' ', ""),
    })).collect()
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

fn module(source: &str) -> Result<Value, Box<dyn Error>> {
    let mut types = serde_json::Map::new();
    for item in syn::parse_file(source)?.items {
        match item {
            syn::Item::Enum(item)
                if matches!(item.vis, syn::Visibility::Public(_)) && encoded(&item.attrs) =>
            {
                let variants: Vec<Value> = item.variants.iter().enumerate().map(|(tag, variant)| {
                    json!({ "tag": tag, "name": variant.ident.to_string(), "fields": fields(&variant.fields) })
                }).collect();
                types.insert(item.ident.to_string(), json!({"variants": variants}));
            }
            syn::Item::Struct(item)
                if matches!(item.vis, syn::Visibility::Public(_)) && encoded(&item.attrs) =>
            {
                types.insert(
                    item.ident.to_string(),
                    json!({"fields": fields(&item.fields)}),
                );
            }
            _ => {}
        }
    }
    Ok(Value::Object(types))
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
                    "token": module(&std::fs::read_to_string(core.join("token.rs"))?)?,
                    "authority": module(&std::fs::read_to_string(core.join("authority.rs"))?)?,
                }
            });
            println!("{}", serde_json::to_string_pretty(&idl)?);
        }
        [command, sources @ ..] if command == "idl-source" && !sources.is_empty() => {
            let mut modules = serde_json::Map::new();
            for source in sources {
                let (name, path) = source.split_once('=').ok_or("expected module=source.rs")?;
                modules.insert(name.to_string(), module(&std::fs::read_to_string(path)?)?);
            }
            println!("{}", serde_json::to_string_pretty(&json!({
                "encoding": "borsh", "version": 1, "modules": modules,
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
