//! Generic [`PUT`] trait defining an interface with a TLS library with which we can:
//! - [`new`] create a client (or server) new initial state + bind buffers
//! - [`progress`] makes a state progress (interacting with the buffers)
//!
//! And specific implementations of PUT for the different PUTs.
use std::{
    any::TypeId,
    cell::RefCell,
    fmt::{Debug, Display, Formatter, Write},
    hash::Hash,
    ops::DerefMut,
    rc::Rc,
};

use serde::{Deserialize, Serialize};

use crate::{
    agent::{AgentDescriptor, AgentName, AgentType, TLSVersion},
    algebra::dynamic_function::TypeShape,
    claims::{ClaimList, GlobalClaimList},
    error::Error,
    io::Stream,
    put_registry::DUMMY_PUT,
};

#[derive(Debug, Copy, Clone, Deserialize, Serialize, Eq, PartialEq, Hash)]
pub struct PutName(pub [char; 10]);

impl Default for PutName {
    fn default() -> Self {
        DUMMY_PUT
    }
}

impl Display for PutName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_iter(self.0))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq, Hash, Default)]
pub struct PutOptions {
    options: Vec<(String, String)>,
}

impl PutOptions {
    pub fn new(options: Vec<(&str, &str)>) -> Self {
        Self {
            options: Vec::from_iter(
                options
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string())),
            ),
        }
    }
}

impl PutOptions {
    pub fn get_option(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(found_key, _value)| -> bool { found_key == key })
            .map(|(_key, value)| value.as_str())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq, Hash, Default)]
pub struct PutDescriptor {
    pub name: PutName,
    pub options: PutOptions,
}

/// Static configuration for creating a new agent state for the PUT
#[derive(Clone)]
pub struct PutConfig {
    pub descriptor: PutDescriptor,
    pub typ: AgentType,
    pub tls_version: TLSVersion,
    pub claims: GlobalClaimList,
    pub authenticate_peer: bool,
    pub extract_deferred: Rc<RefCell<Option<TypeShape>>>,
}

pub trait Put: Stream + Drop + 'static {
    /// Create a new agent state for the PUT + set up buffers/BIOs
    fn new(agent: &AgentDescriptor, config: PutConfig) -> Result<Self, Error>
    where
        Self: Sized;
    /// Process incoming buffer, internal progress, can fill in output buffer
    fn progress(&mut self, agent_name: &AgentName) -> Result<(), Error>;
    /// In-place reset of the state
    fn reset(&mut self, agent_name: AgentName) -> Result<(), Error>;
    fn config(&self) -> &PutConfig;
    /// Register a new claim for agent_name
    #[cfg(feature = "claims")]
    fn register_claimer(&mut self, agent_name: AgentName);
    /// Remove all claims in self
    #[cfg(feature = "claims")]
    fn deregister_claimer(&mut self);
    /// Propagate agent changes to the PUT
    fn rename_agent(&mut self, agent_name: AgentName) -> Result<(), Error>;
    /// Returns a textual representation of the state in which self is
    fn describe_state(&self) -> &str;
    /// Checks whether the Put is in a good state
    fn is_state_successful(&self) -> bool;
    /// Returns a textual representation of the version of the PUT used by self
    fn version() -> &'static str
    where
        Self: Sized;
    /// Make the PUT used by self determimistic in the future by making its PRNG "deterministic"
    fn make_deterministic()
    where
        Self: Sized;

    /// checks whether a agent is reusable with the descriptor
    fn is_reusable_with(&self, other: &AgentDescriptor) -> bool {
        let config = self.config();
        config.typ == other.typ && config.tls_version == other.tls_version
    }

    fn shutdown(&mut self) -> String;
}
