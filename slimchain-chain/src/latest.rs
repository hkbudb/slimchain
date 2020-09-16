use crate::block::{BlockHeader, BlockTrait};
use arc_swap::{ArcSwap, Guard};
use once_cell::sync::OnceCell;
use slimchain_common::{
    basic::{BlockHeight, H256},
    error::{Context as _, Result},
};
use std::sync::Arc;

static LATEST_BLOCK_HEADER: OnceCell<ArcSwap<BlockHeader>> = OnceCell::new();

pub fn set_latest_block_header(block: &impl BlockTrait) {
    let header = Arc::new(block.block_header().clone());
    LATEST_BLOCK_HEADER
        .get_or_init(|| ArcSwap::new(header.clone()))
        .store(header);
}

fn get_latest_block_header_inner() -> Result<Guard<'static, Arc<BlockHeader>>> {
    Ok(LATEST_BLOCK_HEADER
        .get()
        .context("Failed to load the latest block header.")?
        .load())
}

pub fn get_latest_block_header() -> Result<Arc<BlockHeader>> {
    let header = get_latest_block_header_inner()?;
    Ok(Guard::into_inner(header))
}

pub fn get_latest_block_height() -> Result<BlockHeight> {
    let header = get_latest_block_header_inner()?;
    Ok(header.height)
}

pub fn get_latest_block_height_and_state_root() -> Result<(BlockHeight, H256)> {
    let header = get_latest_block_header_inner()?;
    Ok((header.height, header.state_root))
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn reset_latest_block_header() {
    let ptr = core::mem::transmute::<
        *const OnceCell<ArcSwap<BlockHeader>>,
        *mut OnceCell<ArcSwap<BlockHeader>>,
    >(&LATEST_BLOCK_HEADER);
    (*ptr).take();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test() {
        assert!(get_latest_block_header().is_err());
        let mut block = crate::consensus::raft::Block::genesis_block();
        block.block_header_mut().height = 2.into();
        set_latest_block_header(&block);
        assert_eq!(
            block.block_header(),
            get_latest_block_header().unwrap().as_ref()
        );
        assert_eq!(BlockHeight::from(2), get_latest_block_height().unwrap());
        assert_eq!(
            (BlockHeight::from(2), H256::zero()),
            get_latest_block_height_and_state_root().unwrap()
        );

        block.block_header_mut().height = 3.into();
        set_latest_block_header(&block);
        assert_eq!(
            block.block_header(),
            get_latest_block_header().unwrap().as_ref()
        );
        assert_eq!(BlockHeight::from(3), get_latest_block_height().unwrap());
        assert_eq!(
            (BlockHeight::from(3), H256::zero()),
            get_latest_block_height_and_state_root().unwrap()
        );

        unsafe {
            reset_latest_block_header();
        }
        assert!(get_latest_block_header().is_err());
    }
}
