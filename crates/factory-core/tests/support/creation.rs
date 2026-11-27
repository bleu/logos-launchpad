use factory_core::{LbpCreationStage, Message};
use lee_core::{
    account::Actor,
    program::{ReceiveInput, Response, StateReply, SyncMessage},
};

pub struct Creation {
    factory: Actor,
    namespace: Actor,
    continuations: [SyncMessage; 3],
}

impl Creation {
    pub fn new(input: &ReceiveInput, response: &Response) -> Self {
        let stages = [
            LbpCreationStage::Minted,
            LbpCreationStage::Allocated,
            LbpCreationStage::PoolPrepared,
        ];
        let (namespace, program, expected) = match borsh::from_slice(&input.message).unwrap() {
            Message::CreateFactoryPool {
                namespace,
                curve_program_id,
                creator,
                now,
                ..
            } => (
                namespace,
                curve_program_id,
                stages.map(|_| Message::ContinueCreation { creator, now }),
            ),
            Message::CreateFixedPriceLbp { inputs } => (
                inputs.namespace,
                inputs.namespace_program_id,
                stages.map(|expected_stage| Message::ContinueFixedPriceCreation {
                    expected_stage,
                    creator: inputs.creator,
                    now: inputs.now,
                }),
            ),
            Message::CreateWeightedLbp { inputs } => (
                inputs.namespace,
                inputs.namespace_program_id,
                stages.map(|expected_stage| Message::ContinueWeightedCreation {
                    expected_stage,
                    creator: inputs.creator,
                }),
            ),
            _ => panic!("expected a create message"),
        };
        let (read, continuations) = response.sync_messages.split_first().unwrap();
        let namespace_actor =
            Actor::new(curve_core::compute_config_pda(namespace, program), program);
        assert_eq!(read.to, namespace_actor);
        assert_eq!(
            borsh::from_slice::<curve_core::Message>(&read.message).unwrap(),
            curve_core::Message::ReadNamespace { namespace }
        );
        let continuations: [SyncMessage; 3] = continuations.to_vec().try_into().unwrap();
        for (call, expected) in continuations.iter().zip(expected) {
            assert_eq!(call.to, input.receiver);
            assert!(call.pda_seeds.is_empty());
            assert_eq!(call.message, borsh::to_vec(&expected).unwrap());
        }
        Self {
            factory: input.receiver,
            namespace: read.to,
            continuations,
        }
    }

    pub fn namespace_reply(&self, response: &Response, bytes: Vec<u8>) -> ReceiveInput {
        ReceiveInput {
            receiver: self.factory,
            from: Some(self.namespace),
            is_authorized: false,
            pre_state: response.post_state.clone().unwrap(),
            message: borsh::to_vec(&StateReply {
                state: bytes.into(),
            })
            .unwrap(),
        }
    }

    pub fn assert_rejects_external_continuation(&self, previous: &Response) {
        let mut input = self.continuation(previous, LbpCreationStage::Minted);
        for sender in [
            None,
            Some(Actor::new(
                self.namespace.account_id,
                self.factory.program_account_id,
            )),
            Some(Actor::new(
                self.factory.account_id,
                self.namespace.program_account_id,
            )),
        ] {
            input.from = sender;
            assert_eq!(
                factory_core::handle(&input).err(),
                Some(factory_core::Error::Unauthorized)
            );
        }
    }

    pub fn continuation(&self, previous: &Response, stage: LbpCreationStage) -> ReceiveInput {
        let index = match stage {
            LbpCreationStage::Minted => 0,
            LbpCreationStage::Allocated => 1,
            LbpCreationStage::PoolPrepared => 2,
            LbpCreationStage::Complete => panic!("creation is already complete"),
        };
        let call = &self.continuations[index];
        ReceiveInput {
            receiver: call.to,
            from: Some(self.factory),
            is_authorized: false,
            pre_state: previous.post_state.clone().unwrap(),
            message: call.message.clone(),
        }
    }
}
