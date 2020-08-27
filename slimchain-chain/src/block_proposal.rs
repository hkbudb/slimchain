use crate::{
    block::{BlockTrait, BlockTxList},
    loader::{BlockLoaderTrait, TxLoaderTrait},
};
use serde::{
    de::{self, Deserializer, MapAccess, SeqAccess, Visitor},
    ser::{SerializeStruct, Serializer},
    Deserialize, Serialize,
};
use slimchain_common::{
    error::{ensure, Result},
    rw_set::TxWriteData,
    tx::TxTrait,
};
use slimchain_tx_state::{TxStateView, TxTrieDiff, TxWriteSetTrie};
use std::{fmt, iter::FromIterator, marker::PhantomData};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct BlockProposal<Block: BlockTrait, Tx: TxTrait> {
    block: Block,
    txs: Vec<Tx>,
    trie: BlockProposalTrie,
}

impl<Block: BlockTrait, Tx: TxTrait> BlockProposal<Block, Tx> {
    pub fn new(block: Block, txs: Vec<Tx>, trie: BlockProposalTrie) -> Self {
        Self { block, txs, trie }
    }

    pub fn from_existing_block(
        block: Block,
        block_loader: &impl BlockLoaderTrait<Block>,
        tx_loader: &impl TxLoaderTrait<Tx>,
        state_view: &impl TxStateView,
    ) -> Result<Self> {
        let height = block.block_height();
        ensure!(
            !height.is_zero(),
            "Cannot create the block proposal from the genesis block."
        );

        let txs = block.tx_list().to_txs(tx_loader)?;

        let mut writes = TxWriteData::default();
        for tx in &txs {
            writes.merge(tx.tx_writes());
        }

        let prev_state_root = {
            let prev_blk = block_loader.get_block(height.prev_height())?;
            prev_blk.state_root()
        };
        let trie = TxWriteSetTrie::new(state_view, prev_state_root, &writes)?;

        Ok(Self::new(block, txs, BlockProposalTrie::Trie(trie)))
    }

    pub fn get_block(&self) -> &Block {
        &self.block
    }

    pub fn get_block_mut(&mut self) -> &mut Block {
        &mut self.block
    }

    pub fn get_txs(&self) -> &[Tx] {
        &self.txs
    }

    pub fn get_trie(&self) -> &BlockProposalTrie {
        &self.trie
    }

    pub fn unpack(self) -> (Block, Vec<Tx>) {
        (self.block, self.txs)
    }
}

impl<Block: BlockTrait + Serialize, Tx: TxTrait + Serialize> Serialize
    for BlockProposal<Block, Tx>
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut ser_block = self.block.clone();
        ser_block.tx_list_mut().clear();
        let mut state = serializer.serialize_struct("BlockProposal", 3)?;
        state.serialize_field("block", &ser_block)?;
        state.serialize_field("txs", &self.txs)?;
        state.serialize_field("trie", &self.trie)?;
        state.end()
    }
}

impl<'de, Block: BlockTrait + Deserialize<'de>, Tx: TxTrait + Deserialize<'de>> Deserialize<'de>
    for BlockProposal<Block, Tx>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Block,
            Txs,
            Trie,
        }

        struct BlockProposalVisitor<Block: BlockTrait, Tx: TxTrait> {
            _marker: PhantomData<(Block, Tx)>,
        }

        impl<Block: BlockTrait, Tx: TxTrait> Default for BlockProposalVisitor<Block, Tx> {
            fn default() -> Self {
                Self {
                    _marker: PhantomData,
                }
            }
        }

        impl<'de, Block: BlockTrait + Deserialize<'de>, Tx: TxTrait + Deserialize<'de>> Visitor<'de>
            for BlockProposalVisitor<Block, Tx>
        {
            type Value = BlockProposal<Block, Tx>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct BlockProposal")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut block: Block = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let txs: Vec<Tx> = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let trie: BlockProposalTrie = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                *block.tx_list_mut() = BlockTxList::from_iter(txs.iter());
                Ok(BlockProposal::new(block, txs, trie))
            }

            fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut block: Option<Block> = None;
                let mut txs: Option<Vec<Tx>> = None;
                let mut trie: Option<BlockProposalTrie> = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Block => {
                            if block.is_some() {
                                return Err(de::Error::duplicate_field("block"));
                            }
                            block = Some(map.next_value()?);
                        }
                        Field::Txs => {
                            if txs.is_some() {
                                return Err(de::Error::duplicate_field("txs"));
                            }
                            txs = Some(map.next_value()?);
                        }
                        Field::Trie => {
                            if trie.is_some() {
                                return Err(de::Error::duplicate_field("trie"));
                            }
                            trie = Some(map.next_value()?);
                        }
                    }
                }
                let mut block = block.ok_or_else(|| de::Error::missing_field("block"))?;
                let txs = txs.ok_or_else(|| de::Error::missing_field("txs"))?;
                let trie = trie.ok_or_else(|| de::Error::missing_field("trie"))?;
                *block.tx_list_mut() = BlockTxList::from_iter(txs.iter());
                Ok(BlockProposal::new(block, txs, trie))
            }
        }

        const FIELDS: &'static [&'static str] = &["block", "txs", "trie"];
        deserializer.deserialize_struct(
            "BlockProposal",
            FIELDS,
            BlockProposalVisitor::<Block, Tx>::default(),
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum BlockProposalTrie {
    Trie(TxWriteSetTrie),
    Diff(TxTrieDiff),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockHeader;
    use slimchain_common::{
        basic::{Address, BlockHeight, H256},
        digest::Digestible,
        rw_set::{TxReadSet, TxWriteData},
        tx::TxTrait,
        tx_req::TxRequest,
    };

    #[test]
    fn test_serde() {
        #[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
        struct DummyTx;

        impl Digestible for DummyTx {
            fn to_digest(&self) -> H256 {
                H256::zero()
            }
        }

        impl TxTrait for DummyTx {
            fn tx_caller(&self) -> Address {
                unreachable!();
            }
            fn tx_input(&self) -> &TxRequest {
                unreachable!();
            }
            fn tx_block_height(&self) -> BlockHeight {
                unreachable!();
            }
            fn tx_state_root(&self) -> H256 {
                unreachable!();
            }
            fn tx_reads(&self) -> &TxReadSet {
                unreachable!();
            }
            fn tx_writes(&self) -> &TxWriteData {
                unreachable!();
            }
            fn verify_sig(&self) -> Result<()> {
                unreachable!();
            }
        }

        #[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
        struct DummyBlock {
            tx_list: BlockTxList,
        }

        impl Digestible for DummyBlock {
            fn to_digest(&self) -> H256 {
                self.tx_list.to_digest()
            }
        }

        impl BlockTrait for DummyBlock {
            fn block_header(&self) -> &BlockHeader {
                unreachable!();
            }
            fn block_header_mut(&mut self) -> &mut BlockHeader {
                unreachable!();
            }
            fn tx_list(&self) -> &BlockTxList {
                &self.tx_list
            }
            fn tx_list_mut(&mut self) -> &mut BlockTxList {
                &mut self.tx_list
            }
        }

        let tx = DummyTx::default();
        let tx_list = BlockTxList::from_iter(std::iter::once(&tx));
        let block = DummyBlock { tx_list };
        let proposal =
            BlockProposal::new(block, vec![tx], BlockProposalTrie::Diff(Default::default()));

        let bin = postcard::to_allocvec(&proposal).unwrap();
        assert_eq!(proposal, postcard::from_bytes(&bin[..]).unwrap());

        let json = serde_json::to_string(&proposal).unwrap();
        assert_eq!(proposal, serde_json::from_str(&json).unwrap());
    }
}
