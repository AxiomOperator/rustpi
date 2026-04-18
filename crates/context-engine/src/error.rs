use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("token budget exceeded: {used} of {budget} tokens")]
    BudgetExceeded { used: u32, budget: u32 },
    #[error("no relevant files found in {0}")]
    NoRelevantFiles(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
