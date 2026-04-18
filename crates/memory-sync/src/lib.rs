pub mod docs;
pub mod error;
pub mod markdown;
pub mod memory;
pub mod personality;
pub mod qdrant;
pub mod sync;
pub mod vault;

pub use docs::{CanonicalDoc, DocMutability, LoadedDoc, VAULT_DOCS};
pub use error::MemorySyncError;
pub use markdown::{DocSection, VaultDoc};
pub use memory::MemoryRecord;
pub use personality::{
    inject_personality, load_personality, PersonalityConfig, PersonalityContext,
    PersonalitySection,
};
pub use sync::{ConflictRecord, ConflictResolution, SyncDirection, SyncEngine, SyncResult};
pub use vault::VaultAccessor;
