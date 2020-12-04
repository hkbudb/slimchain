use crate::view::{
    trie_view::{AccountTrieView, StateTrieView},
    TxStateView,
};
use alloc::format;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{account_data_to_digest, Address, Nonce, H256},
    collections::HashMap,
    digest::Digestible,
    error::{ensure, Result},
    rw_set::TxWriteData,
};
use slimchain_merkle_trie::prelude::*;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct AccountWriteSetTrie {
    pub nonce: Nonce,
    pub code_hash: H256,
    pub state_trie: PartialTrie,
}

impl AccountWriteSetTrie {
    pub fn acc_hash(&self) -> H256 {
        account_data_to_digest(
            self.nonce.to_digest(),
            self.code_hash,
            self.state_trie.root_hash(),
        )
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TxWriteSetTrie {
    pub(crate) main_trie: PartialTrie,
    pub(crate) acc_tries: HashMap<Address, AccountWriteSetTrie>,
}

impl TxWriteSetTrie {
    pub fn new(
        state_view: &impl TxStateView,
        root_address: H256,
        writes: &TxWriteData,
    ) -> Result<Self> {
        let mut acc_tries: HashMap<Address, AccountWriteSetTrie> = HashMap::new();
        let mut main_read_ctx =
            ReadTrieContext::new(AccountTrieView::new(state_view), root_address);

        for (acc_address, acc_write) in writes.iter() {
            let acc_data = main_read_ctx.read(acc_address)?;
            let acc_state_root = acc_data.map(|acc| acc.acc_state_root).unwrap_or_default();
            let state_partial_trie = if acc_write.reset_values {
                PartialTrie::from_root_hash(acc_state_root)
            } else {
                let mut value_read_ctx = ReadTrieContext::new(
                    StateTrieView::new(state_view, *acc_address),
                    acc_state_root,
                );

                for key in acc_write.values.keys() {
                    value_read_ctx.read(key)?;
                }

                value_read_ctx.into_proof().into()
            };

            let acc_proof = AccountWriteSetTrie {
                nonce: acc_data.map(|acc| acc.nonce).unwrap_or_default(),
                code_hash: acc_data.map(|acc| acc.code.to_digest()).unwrap_or_default(),
                state_trie: state_partial_trie,
            };

            acc_tries.insert(*acc_address, acc_proof);
        }

        Ok(Self {
            main_trie: main_read_ctx.into_proof().into(),
            acc_tries,
        })
    }

    pub fn verify(&self, state_root: H256) -> Result<()> {
        for (acc_address, acc_trie) in self.acc_tries.iter() {
            let acc_hash = acc_trie.acc_hash();
            let main_trie_acc_hash = self.main_trie.value_hash(acc_address);

            ensure!(
                main_trie_acc_hash == Some(acc_hash),
                "TxWriteSetTrie: Invalid account hash (address: {}, expect: {:?}, actual: {:?}).",
                acc_address,
                main_trie_acc_hash,
                Some(acc_hash)
            );
        }

        let main_trie_root = self.main_trie.root_hash();
        ensure!(
            main_trie_root == state_root,
            "TxWriteSetTrie: Invalid state root (expect: {}, actual: {}).",
            state_root,
            main_trie_root
        );

        Ok(())
    }

    #[cfg(feature = "draw")]
    pub fn to_graph(
        &self,
        name: impl alloc::string::ToString,
    ) -> slimchain_merkle_trie::draw::MultiGraph {
        use alloc::{string::ToString, vec};
        use slimchain_merkle_trie::draw::*;

        let name = name.to_string();
        let mut graph = MultiGraph::new(name.clone());
        let main_trie_graph =
            Graph::from_partial_trie(format!("{}_main_trie", name), &self.main_trie);
        graph.add_sub_graph(&main_trie_graph);

        for (i, (acc_addr, acc_trie)) in self.acc_tries.iter().enumerate() {
            let mut acc_trie_graph =
                Graph::from_partial_trie(format!("{}_acc_trie_{}", name, i), &acc_trie.state_trie);
            acc_trie_graph.set_label(format!(
                "addr = {}\nnonce = {}\ncode_hash = {}\nstate_hash = {}\nhash = {}",
                acc_addr,
                acc_trie.nonce,
                acc_trie.code_hash,
                acc_trie.state_trie.root_hash(),
                acc_trie.acc_hash()
            ));
            graph.add_sub_graph(&acc_trie_graph);

            if let Some(parent_id) = main_trie_graph.vertex_dot_id(acc_addr) {
                if let Some(child_id) = acc_trie_graph.vertex_dot_id(NibbleBuf::default()) {
                    graph.add_edge(
                        parent_id,
                        child_id,
                        None,
                        vec![
                            "style=dashed".to_string(),
                            format!("lhead=cluster_{}", acc_trie_graph.get_name()),
                        ],
                    );
                }
            }
        }

        graph
    }

    #[cfg(feature = "draw")]
    pub fn draw(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        use slimchain_merkle_trie::draw::*;
        let graph = self.to_graph("write_set_trie");
        draw_dot(graph.to_dot(false), path)
    }
}
