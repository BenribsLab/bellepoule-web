//! À porter : voir ARCHITECTURE.md (poules : pool, pool_allocator, dispatcher, neo_swapper, smart_swapper, match, score).

pub mod dispatcher;
pub mod neo_swapper;
pub mod pool;
pub mod round;
pub mod stages;

use crate::stage::{Stage, StageKind};

pub use stages::{PoolAllocatorStage, PoolSupervisorStage, SharedRound};

/// Fabrique des phases de ce module (signature fixe, appelée par `rounds::create_stage`).
pub fn new_stage(kind: StageKind, id: u32) -> Box<dyn Stage> {
    match kind {
        StageKind::PoolAllocator => Box::new(PoolAllocatorStage::new(kind, id)),
        _ => Box::new(PoolSupervisorStage::new(kind, id)),
    }
}
