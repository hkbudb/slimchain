#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use alloc::{format, vec::Vec};
use core::cell::{Cell, RefCell};
use evm::backend::Backend as _;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{Address, Code, Nonce, StateKey, StateValue, H160, H256, U256},
    digest::Digestible,
    error::{ensure, Context as _, Error, Result},
    rw_set::{TxReadData, TxWriteData},
    tx::{SignedTxRequest, TxRequest},
};

pub trait Backend {
    fn get_nonce(&self, acc_address: Address) -> Result<Nonce>;
    fn get_code(&self, acc_address: Address) -> Result<Code>;
    fn get_value(&self, acc_address: Address, key: StateKey) -> Result<StateValue>;
}

struct EVMBackend<'a, B: Backend> {
    backend: &'a B,
    reads: RefCell<TxReadData>,
    error: Cell<Option<Error>>,
}

impl<'a, B: Backend> EVMBackend<'a, B> {
    fn new(backend: &'a B) -> Self {
        Self {
            backend,
            reads: RefCell::new(TxReadData::default()),
            error: Cell::new(None),
        }
    }

    fn set_error(&self, err: Error) {
        self.error.replace(Some(err));
    }

    fn check_error(&self) -> Result<()> {
        match self.error.take() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    fn take_reads(&self) -> TxReadData {
        self.reads.replace_with(|_| Default::default())
    }
}

impl<'a, B: Backend> evm::backend::Backend for EVMBackend<'a, B> {
    fn gas_price(&self) -> U256 {
        unimplemented!();
    }
    fn origin(&self) -> H160 {
        unimplemented!();
    }
    fn block_hash(&self, _number: U256) -> H256 {
        unimplemented!();
    }
    fn block_number(&self) -> U256 {
        unimplemented!();
    }
    fn block_coinbase(&self) -> H160 {
        unimplemented!();
    }
    fn block_timestamp(&self) -> U256 {
        unimplemented!();
    }
    fn block_difficulty(&self) -> U256 {
        unimplemented!();
    }
    fn block_gas_limit(&self) -> U256 {
        U256::max_value()
    }
    fn chain_id(&self) -> U256 {
        unimplemented!();
    }
    fn exists(&self, _address: H160) -> bool {
        true
    }
    fn basic(&self, address: H160) -> evm::backend::Basic {
        let address: Address = address.into();
        match self.backend.get_nonce(address) {
            Ok(nonce) => {
                self.reads.borrow_mut().add_nonce(address, nonce);
                evm::backend::Basic {
                    balance: U256::max_value(),
                    nonce: nonce.into(),
                }
            }
            Err(err) => {
                self.set_error(err);
                Default::default()
            }
        }
    }
    fn code_hash(&self, address: H160) -> H256 {
        Code::from(self.code(address)).to_digest()
    }
    fn code_size(&self, address: H160) -> usize {
        self.code(address).len()
    }
    fn code(&self, address: H160) -> Vec<u8> {
        let address: Address = address.into();
        match self.backend.get_code(address) {
            Ok(code) => {
                self.reads.borrow_mut().add_code(address, code.clone());
                code.into()
            }
            Err(err) => {
                self.set_error(err);
                Default::default()
            }
        }
    }
    fn storage(&self, address: H160, index: H256) -> H256 {
        let address: Address = address.into();
        let index: StateKey = index.into();
        match self.backend.get_value(address, index) {
            Ok(value) => {
                self.reads.borrow_mut().add_value(address, index, value);
                value.into()
            }
            Err(err) => {
                self.set_error(err);
                Default::default()
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteOutput {
    pub caller: Address,
    pub input: TxRequest,
    pub reads: TxReadData,
    pub writes: TxWriteData,
}

pub fn execute_tx(signed_tx_req: SignedTxRequest, backend: &impl Backend) -> Result<ExecuteOutput> {
    signed_tx_req.verify().context("Invalid signature.")?;

    let caller = signed_tx_req.caller_address();
    let tx_req = signed_tx_req.tx;

    let evm_backend = EVMBackend::new(backend);
    let evm_config = evm::Config::istanbul();
    let mut executor =
        evm::executor::StackExecutor::new(&evm_backend, usize::max_value(), &evm_config);

    let caller_nonce = Nonce::from(evm_backend.basic(caller.into()).nonce);
    let input_nonce = tx_req.nonce();
    ensure!(
        caller_nonce == input_nonce,
        "Invalid nonce (expected: {}, actual: {}).",
        caller_nonce,
        input_nonce
    );

    let execute_result = match &tx_req {
        TxRequest::Create { code, .. } => executor.transact_create(
            caller.clone().into(),
            U256::zero(),
            code.clone().into(),
            usize::max_value(),
        ),
        TxRequest::Call { address, data, .. } => {
            executor
                .transact_call(
                    caller.clone().into(),
                    address.clone().into(),
                    U256::zero(),
                    data.clone(),
                    usize::max_value(),
                )
                .0
        }
    };

    evm_backend
        .check_error()
        .context("Error when accessing the backend.")?;

    ensure!(
        execute_result.is_succeed(),
        "Failed to execute the tx (reason: {:?}).",
        execute_result
    );

    let mut reads = evm_backend.take_reads();
    let mut writes = TxWriteData::default();

    let (applies, _logs) = executor.deconstruct();
    for apply in applies {
        match apply {
            evm::backend::Apply::Modify {
                address,
                basic,
                code,
                storage,
                reset_storage,
            } => {
                let address = Address::from(address);
                let nonce = Nonce::from(basic.nonce);
                // only record nonce read when the value is updated
                if Some(nonce) == reads.get_nonce(address) {
                    reads.remove_nonce(address);
                } else {
                    writes.add_nonce(address, nonce);
                }
                if let Some(code) = code {
                    writes.add_code(address, Code::from(code));
                }
                if reset_storage {
                    writes.add_reset_values(address);
                }
                for (key, value) in storage {
                    let key = StateKey::from(key);
                    writes.add_value(address, key, StateValue::from(value));
                }
            }
            evm::backend::Apply::Delete { address } => {
                let address = Address::from(address);
                writes.delete_account(address);
            }
        }
    }

    Ok(ExecuteOutput {
        caller,
        input: tx_req,
        reads,
        writes,
    })
}
