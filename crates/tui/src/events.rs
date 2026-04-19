use agent_core::types::AgentEvent;
use crossterm::event::KeyEvent;

pub enum UiEvent {
    AgentEvent(AgentEvent),
    Tick,
    Resize(u16, u16),
    KeyInput(KeyEvent),
}
