use std::collections::HashMap;
use std::fmt;

use itertools::Itertools;

use crate::{
    term::{
        atoms::Variable,
        dynamic_function::{
            make_dynamic, DescribableFunction, DynamicFunction, DynamicFunctionShape, TypeShape,
        },
    },
    trace::ObservedId,
};

use super::atoms::Function;
use crate::agent::AgentName;
use crate::trace::{TlsMessageType, QueryId};

pub type FunctionDefinition = (DynamicFunctionShape, Box<dyn DynamicFunction>);

/// Records a universe of functions.
/// Signatures are containers for types and function symbols. They hold references to the concrete
/// implementations of functions and the types of variables.
pub struct Signature {
    pub functions_by_name: HashMap<&'static str, FunctionDefinition>,
    pub functions_by_typ: HashMap<TypeShape, Vec<FunctionDefinition>>,
    pub functions: Vec<FunctionDefinition>,
    pub types_by_name: HashMap<&'static str, TypeShape>, // LH: Why not owned String as in `function_by_name` or use `static there as well?
}

impl Signature {
    /// Construct a `Signature` from the given [`FunctionDefinitions`]s.
    pub fn new(definitions: Vec<FunctionDefinition>) -> Signature {
        let functions_by_name: HashMap<&'static str, FunctionDefinition> = definitions
            .clone()
            .into_iter()
            .map(|(shape, dynamic_fn)| (shape.name.clone(), (shape, dynamic_fn)))
            .collect();

        let functions_by_typ: HashMap<TypeShape, Vec<FunctionDefinition>> = definitions
            .clone()
            .into_iter()
            .into_group_map_by(|(shape, _dynamic_fn)| shape.return_type);

        let types_by_name: HashMap<&'static str, TypeShape> = definitions
            .clone()
            .into_iter()
            .map(|(shape, _dynamic_fn)| {
                let used_types: Vec<TypeShape> = shape // vector of the argument shapes + return type
                    .argument_types
                    .iter()
                    .copied()
                    .chain(vec![shape.return_type])
                    .collect::<Vec<TypeShape>>();
                used_types
            })
            .unique()
            .flatten() // LH: I do not understand this, what is the intended returned TypeShape in the HashMap?
            .map(|typ| (typ.name, typ.clone()))
            .collect();

        Signature {
            functions_by_name,
            functions_by_typ,
            functions: definitions,
            types_by_name,
        }
    }

    /// Create a new [`Functions`] distinct from all existing [`Functions`]s.
    ///
    pub fn new_function<F: 'static, Types>(f: &'static F) -> Function
    where
        F: DescribableFunction<Types>,
    {
        let (shape, dynamic_fn) = make_dynamic(f);
        let func = Function::new(shape.clone(), dynamic_fn.clone());
        func
    }

    pub fn new_var<T: 'static>(query_id: QueryId) -> Variable {
        let type_shape = TypeShape::of::<T>();
        Variable::new(type_shape, query_id)
    }

    pub fn new_var_by_type_id(
        type_shape: TypeShape,
        agent_name: AgentName,
        tls_message_type: Option<TlsMessageType>,
        counter: u16,
    ) -> Variable {
        let query_id = QueryId {
            agent_name,
            tls_message_type,
            counter,
        };
        Variable::new(type_shape, query_id)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Signature{{{:?}}}", self)
    }
}

#[macro_export]
macro_rules! define_signature {   // LH: To document somewhere: it does not seem that your signature and the way you (de)serialize are robust to function name modifications (?)
    ($name_signature:ident, $($f:path)+) => {
        use once_cell::sync::Lazy;
        use crate::term::signature::Signature;
        /// Signature which contains all functions defined in the `tls` module.
        pub static $name_signature: Lazy<Signature> = Lazy::new(|| {
            let definitions = vec![
                $(crate::term::dynamic_function::make_dynamic(&$f)),*
            ];
            Signature::new(definitions)
        });
    };
}
