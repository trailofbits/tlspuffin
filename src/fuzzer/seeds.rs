use rustls::internal::msgs::base::Payload;
use rustls::internal::msgs::enums::Compression;
use rustls::internal::msgs::handshake::{ClientExtension, Random, ServerExtension, SessionID};
use rustls::{CipherSuite, ProtocolVersion};

use crate::term::{
    Signature, Term
};
use crate::term::op_impl::{
    op_application_data, op_change_cipher_spec, op_client_hello, op_server_hello,
};
use crate::trace::{Action, InputAction, OutputAction, Step, Trace, TraceContext};
use crate::agent::AgentName;

pub fn seed_successful(ctx: &mut TraceContext) -> (AgentName, AgentName, Trace) {
    let client_openssl = ctx.new_openssl_agent(false);
    let server_openssl = ctx.new_openssl_agent(true);

    let mut sig = Signature::default();
    let op_client_hello = sig.new_op(&op_client_hello);
    let op_server_hello = sig.new_op(&op_server_hello);
    let op_change_cipher_spec = sig.new_op(&op_change_cipher_spec);
    //let op_encrypted_certificate = sig.new_op(&op_encrypted_certificate);
    //let op_certificate = sig.new_op(&op_certificate);
    let op_application_data = sig.new_op(&op_application_data);

    (client_openssl, server_openssl, Trace {
        steps: vec![
            Step {
                agent: client_openssl,
                action: Action::Output(OutputAction { id: 0 }),
            },
            // Client Hello Client -> Server
            Step {
                agent: server_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_client_hello,
                        args: vec![
                            Term::Variable(sig.new_var::<ProtocolVersion>((0, 0))),
                            Term::Variable(sig.new_var::<Random>((0, 0))),
                            Term::Variable(sig.new_var::<SessionID>((0, 0))),
                            Term::Variable(sig.new_var::<Vec<CipherSuite>>((0, 0))),
                            Term::Variable(sig.new_var::<Vec<Compression>>((0, 0))),
                            Term::Variable(sig.new_var::<Vec<ClientExtension>>((0, 0))),
                        ],
                    },
                }),
            },
            Step {
                agent: server_openssl,
                action: Action::Output(OutputAction { id: 1 }),
            },
            // Server Hello Server -> Client
            Step {
                agent: client_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_server_hello,
                        args: vec![
                            Term::Variable(sig.new_var::<ProtocolVersion>((1, 0))),
                            Term::Variable(sig.new_var::<Random>((1, 0))),
                            Term::Variable(sig.new_var::<SessionID>((1, 0))),
                            Term::Variable(sig.new_var::<CipherSuite>((1, 0))),
                            Term::Variable(sig.new_var::<Compression>((1, 0))),
                            Term::Variable(sig.new_var::<Vec<ServerExtension>>((1, 0))),
                        ],
                    },
                }),
            },
            // CCS Server -> Client
            Step {
                agent: client_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_change_cipher_spec.clone(),
                        args: vec![],
                    },
                }),
            },
            // Encrypted Extensions Server -> Client
            Step {
                agent: client_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_application_data.clone(),
                        args: vec![Term::Variable(sig.new_var::<Payload>((1, 2)))],
                    },
                }),
            },
            // Certificate Server -> Client
            Step {
                agent: client_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_application_data.clone(),
                        args: vec![Term::Variable(sig.new_var::<Payload>((1, 3)))],
                    },
                }),
            },
            // Certificate Verify Server -> Client
            Step {
                agent: client_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_application_data.clone(),
                        args: vec![Term::Variable(sig.new_var::<Payload>((1, 4)))],
                    },
                }),
            },
            // Finish Server -> Client
            Step {
                agent: client_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_application_data.clone(),
                        args: vec![Term::Variable(sig.new_var::<Payload>((1, 5)))],
                    },
                }),
            },
            Step {
                agent: client_openssl,
                action: Action::Output(OutputAction { id: 2 }),
            },
            /*
            // CCS Client -> Server
            Step {
                agent: server_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_change_cipher_spec.clone(),
                        args: vec![],
                    },
                }),
            },*/
            // todo missing:
            //      CCS Client -> Server
            // Finished Client -> Server
            Step {
                agent: server_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_application_data.clone(),
                        args: vec![Term::Variable(sig.new_var::<Payload>((2, 0)))],
                    },
                }),
            },
            Step {
                agent: server_openssl,
                action: Action::Output(OutputAction { id: 3 }),
            },
            Step {
                agent: client_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_application_data.clone(),
                        args: vec![Term::Variable(sig.new_var::<Payload>((3, 0)))],
                    },
                }),
            },
            Step {
                agent: client_openssl,
                action: Action::Input(InputAction {
                    recipe: Term::Application {
                        op: op_application_data.clone(),
                        // todo can we express this with projections?
                        args: vec![Term::Variable(sig.new_var::<Payload>((3, 1)))],
                    },
                }),
            },
        ],
    })
}
