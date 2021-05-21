use core::fmt;

use serde::{Deserialize, Serialize};

use crate::io::{OpenSSLStream, Stream};

#[derive(Serialize, Deserialize, Copy, Clone, Debug)]
pub struct AgentName(u8);

impl AgentName {
    pub fn new(last_name: &AgentName) -> AgentName {
        AgentName(last_name.0 + 1)
    }

    pub fn none() -> AgentName {
        NONE_AGENT
    }
}

const NONE_AGENT: AgentName = AgentName(0u8);

impl fmt::Display for AgentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialEq for AgentName {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

pub struct Agent {
    pub name: AgentName,
    pub stream: Box<dyn Stream>,
}

impl Agent {
    pub fn new_openssl(last_name: &AgentName, server: bool) -> Self {
        Self::from_stream(last_name, Box::new(OpenSSLStream::new(server)))
    }

    pub fn from_stream(last_name: &AgentName, stream: Box<dyn Stream>) -> Agent {
        Agent {
            name: AgentName::new(&last_name),
            stream,
        }
    }
}
