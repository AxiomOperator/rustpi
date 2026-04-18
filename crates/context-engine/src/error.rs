use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("token budget exceeded: used {used} of {budget} tokens")]
    BudgetExceeded { used: u32, budget: u32 },
    #[error("no relevant files found in '{0}'")]
    NoRelevantFiles(String),
    #[error("file too large to include: {path} ({size_bytes} bytes)")]
    FileTooLarge { path: String, size_bytes: u64 },
    #[error("working set is empty")]
    EmptyWorkingSet,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("context engine error: {0}")]
    Other(String),
}
