use slimchain_common::{
    basic::{AccountData, Address, Code, Nonce, StateKey, StateValue, H256},
    ed25519::Keypair,
    error::Result,
    tx::{RawTx, SignedTx},
};
use slimchain_merkle_trie::prelude::*;
use slimchain_tx_engine::{TxEngineWorker, TxTask};
use slimchain_tx_executor::execute_tx;
use slimchain_tx_state::{
    trie_view::{AccountTrieView, StateTrieView},
    TxStateView,
};

struct ExecutorBackend<'a, StateView: TxStateView + ?Sized> {
    state_view: &'a StateView,
    state_root: H256,
}

impl<'a, StateView: TxStateView + ?Sized> ExecutorBackend<'a, StateView> {
    fn new(state_view: &'a StateView, state_root: H256) -> Self {
        Self {
            state_view,
            state_root,
        }
    }

    fn map_acc_data<T>(
        &self,
        acc_address: Address,
        default: impl FnOnce() -> T,
        f: impl FnOnce(&AccountData) -> T,
    ) -> Result<T> {
        let view = AccountTrieView::new(self.state_view);
        let acc_data = read_trie(&view, self.state_root, &acc_address)?.0;
        Ok(acc_data.as_ref().map_or_else(default, f))
    }
}

impl<'a, StateView: TxStateView + ?Sized> slimchain_tx_executor::Backend
    for ExecutorBackend<'a, StateView>
{
    fn get_nonce(&self, acc_address: Address) -> Result<Nonce> {
        self.map_acc_data(acc_address, Default::default, |d| d.nonce)
    }

    fn get_code(&self, acc_address: Address) -> Result<Code> {
        self.map_acc_data(acc_address, Default::default, |d| d.code.clone())
    }

    fn get_value(&self, acc_address: Address, key: StateKey) -> Result<StateValue> {
        let acc_state_root = self.map_acc_data(acc_address, H256::zero, |d| d.acc_state_root)?;

        let view = StateTrieView::new(self.state_view, acc_address);
        let value = read_trie(&view, acc_state_root, &key)?
            .0
            .unwrap_or_default();
        Ok(value)
    }
}

pub struct SimpleTxEngineWorker {
    keypair: Keypair,
}

impl TxEngineWorker for SimpleTxEngineWorker {
    type Output = SignedTx;

    fn execute(&self, task: TxTask) -> Result<Self::Output> {
        let backend = ExecutorBackend::new(task.state_view.as_ref(), task.state_root);
        let output = execute_tx(task.signed_tx_req, &backend)?;

        let raw_tx = RawTx {
            caller: output.caller,
            input: output.input,
            block_height: task.block_height,
            state_root: task.state_root,
            reads: output.reads.to_set(),
            writes: output.writes,
        };

        Ok(raw_tx.sign(&self.keypair))
    }
}

impl SimpleTxEngineWorker {
    pub fn new(keypair: Keypair) -> Self {
        Self { keypair }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use slimchain_common::{
        basic::U256,
        ed25519::Keypair,
        tx::TxTrait,
        tx_req::{caller_address_from_pk, TxRequest},
    };
    use slimchain_tx_engine::{TxEngine, TxTask, TxTaskOutput};
    use slimchain_tx_state::{MemTxState, TxProposal};
    use slimchain_utils::{
        contract::{contract_address, Contract, Token},
        init_tracing_for_test,
    };
    use std::path::PathBuf;

    #[test]
    fn test() {
        init_tracing_for_test();

        let mut states = MemTxState::new();

        let contract_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("contracts/build/contracts/SimpleStorage.json");
        let contract = Contract::from_json_file(&contract_file).unwrap();

        let mut rng = rand::rngs::StdRng::seed_from_u64(1u64);
        let keypair = Keypair::generate(&mut rng);
        let caller_address = caller_address_from_pk(&keypair.public);
        let contract_address = contract_address(caller_address, U256::from(0).into());

        let task_engine = TxEngine::new(2, || {
            let mut rng = rand::rngs::StdRng::seed_from_u64(1u64);
            Box::new(SimpleTxEngineWorker::new(Keypair::generate(&mut rng)))
        });

        let tx_req1 = TxRequest::Create {
            nonce: U256::from(0).into(),
            code: contract.code().clone(),
        };
        let signed_tx_req1 = tx_req1.sign(&keypair);

        let task1 = TxTask::new(
            1.into(),
            states.state_view(),
            states.state_root(),
            signed_tx_req1,
        );
        task_engine.push_task(task1);
        let TxTaskOutput {
            tx_proposal:
                TxProposal {
                    tx: tx1,
                    write_trie: write_trie1,
                },
            ..
        } = task_engine.pop_or_wait_result();
        assert!(write_trie1.verify(states.state_root()).is_ok());
        assert!(tx1.verify_sig().is_ok());

        assert!(tx1
            .raw_tx
            .writes
            .get(&contract_address)
            .unwrap()
            .values
            .iter()
            .any(|(_k, v)| v.to_low_u64_be() == 42));
        states.apply_writes(&tx1.raw_tx.writes).unwrap();

        let tx_req2 = TxRequest::Call {
            address: contract_address,
            nonce: U256::from(1).into(),
            data: contract
                .encode_tx_input(
                    "set",
                    &[Token::Uint(U256::from(1)), Token::Uint(U256::from(43))],
                )
                .unwrap(),
        };
        let signed_tx_req2 = tx_req2.sign(&keypair);

        let task2 = TxTask::new(
            2.into(),
            states.state_view(),
            states.state_root(),
            signed_tx_req2,
        );
        task_engine.push_task(task2);
        let TxTaskOutput {
            tx_proposal:
                TxProposal {
                    tx: tx2,
                    write_trie: write_trie2,
                },
            ..
        } = task_engine.pop_or_wait_result();
        assert!(write_trie2.verify(states.state_root()).is_ok());
        assert!(tx2.verify_sig().is_ok());

        assert!(tx2
            .raw_tx
            .writes
            .get(&contract_address)
            .unwrap()
            .values
            .iter()
            .any(|(_k, v)| v.to_low_u64_be() == 43));
    }
}
