use std::marker::PhantomData;

use libafl::{
    bolts::{
        rands::Rand,
        tuples::{tuple_list, tuple_list_type, Named},
    },
    corpus::Corpus,
    mutators::{MutationResult, Mutator},
    state::{HasCorpus, HasMaxSize, HasMetadata, HasRand},
    Error,
};

use crate::fuzzer::mutations_util::*;
use crate::term::dynamic_function::DynamicFunction;
use crate::term::signature::FunctionDefinition;
use crate::term::{Subterms, Term};
use crate::tls::SIGNATURE;
use crate::trace::Trace;
use crate::mutator;

pub fn trace_mutations<R, C, S>() -> tuple_list_type!(
       RepeatMutator<R, S>,
       SkipMutator<R, S>,
       ReplaceReuseMutator<R, S>,
       ReplaceMatchMutator<R, S>,
       RemoveAndLiftMutator<R, S>,
       SwapMutator<R,S>
   )
where
    S: HasCorpus<C, Trace> + HasMetadata + HasMaxSize + HasRand<R>,
    C: Corpus<Trace>,
    R: Rand,
{
    tuple_list!(
        RepeatMutator::new(),
        SkipMutator::new(),
        ReplaceReuseMutator::new(),
        ReplaceMatchMutator::new(),
        RemoveAndLiftMutator::new(),
        SwapMutator::new(),
    )
}

mutator! {
    /// SWAP: Swaps a sub-term with a different sub-term which is part of the trace
    /// (such that types match).
    SwapMutator,
    fn mutate(
        &mut self,
        state: &mut S,
        trace: &mut Trace,
        _stage_idx: i32,
    ) -> Result<MutationResult, Error> {
        let rand = state.rand_mut();

        if let Some((term_a, trace_path_a)) = choose(trace, rand) {
            if let Some(trace_path_b) = choose_term_path_filtered(
                trace,
                |term: &Term| term.get_type_shape() == term_a.get_type_shape(),
                rand,
            ) {
                let term_a_cloned = term_a.clone();

                let term_b = find_term_mut(trace, &trace_path_b).unwrap();
                let term_b_cloned = term_b.clone();
                term_b.mutate(term_a_cloned);

                let trace_a_mut = find_term_mut(trace, &trace_path_a).unwrap();
                trace_a_mut.mutate(term_b_cloned);

                return Ok(MutationResult::Mutated);
            }
        }

        Ok(MutationResult::Skipped)
    }
}

mutator! {
    /// REMOVE AND LIFT: Removes a sub-term from a term and attaches orphaned children to the parent
    /// (such that types match). This only works if there is only a single child.
    RemoveAndLiftMutator,
     fn mutate(
        &mut self,
        state: &mut S,
        trace: &mut Trace,
        _stage_idx: i32,
    ) -> Result<MutationResult, Error> {
        let rand = state.rand_mut();

        // Check whether there are grand_subterms with the same shape as a subterm.
        // If we find such a term, then we can remove the subterm and lift the children to the `term`.
        let filter = |term: &Term| match term {
            Term::Variable(_) => false,
            Term::Application(_, subterms) => subterms
                .find_subterm(|subterm| match subterm {
                    Term::Variable(_) => false,
                    Term::Application(_, grand_subterms) => {
                        grand_subterms.find_subterm_same_shape(subterm).is_some()
                    }
                })
                .is_some(),
        };
        if let Some(mut to_mutate) = choose_term_filtered_mut(trace, rand, filter) {
            match &mut to_mutate {
                Term::Variable(_) => {
                    // never reached as `filter` returns false for variables
                    Ok(MutationResult::Skipped)
                }
                Term::Application(_, ref mut subterms) => {
                    if let Some(((subterm_index, _), grand_subterm)) = choose_iter(
                        subterms.filter_grand_subterms(|subterm, grand_subterm| {
                            subterm.get_type_shape() == grand_subterm.get_type_shape()
                        }),
                        rand,
                    ) {
                        subterms.push(grand_subterm.clone());
                        subterms.swap_remove(subterm_index);
                        return Ok(MutationResult::Mutated);
                    }

                    Ok(MutationResult::Skipped)
                }
            }
        } else {
            Ok(MutationResult::Skipped)
        }
    }
}

mutator! {
    /// REPLACE-MATCH: Replaces a function symbol with a different one (such that types match).
    /// An example would be to replace a constant with another constant or the binary function
    /// fn_add with fn_sub.
    ReplaceMatchMutator,
    fn mutate(
        &mut self,
        state: &mut S,
        trace: &mut Trace,
        _stage_idx: i32,
    ) -> Result<MutationResult, Error> {
        let rand = state.rand_mut();

        if let Some(mut to_mutate) =
            choose_term_filtered_mut(trace, rand, |term| matches!(term, Term::Application(_, _)))
        {
            match &mut to_mutate {
                Term::Variable(_) => {
                    // never reached as `filter` returns false for variables
                    Ok(MutationResult::Skipped)
                }
                Term::Application(func_mut, _) => {
                    if let Some((shape, dynamic_fn)) = choose_iter_filtered(
                        &SIGNATURE.functions,
                        |(shape, dynamic_fn)| {
                            func_mut.shape() != shape // do not mutate if we change the same funciton
                                && func_mut.shape().return_type == shape.return_type
                                && func_mut.shape().argument_types == shape.argument_types
                        },
                        rand,
                    ) {
                        func_mut.change_function(shape.clone(), dynamic_fn.clone());
                        Ok(MutationResult::Mutated)
                    } else {
                        Ok(MutationResult::Skipped)
                    }
                }
            }
        } else {
            Ok(MutationResult::Skipped)
        }
    }
}

mutator! {
    /// REPLACE-REUSE: Replaces a sub-term with a different sub-term which is part of the trace
    /// (such that types match). The new sub-term could come from another step which has a different recipe term.
    ReplaceReuseMutator,
    // todo make sure that we do not replace a term with itself (performance improvement)
    fn mutate(
        &mut self,
        state: &mut S,
        trace: &mut Trace,
        _stage_idx: i32,
    ) -> Result<MutationResult, Error> {
        let rand = state.rand_mut();
        if let Some(replacement) = choose_term(trace, rand).cloned() {
            if let Some(to_replace) = choose_term_filtered_mut(trace, rand, |term: &Term| {
                term.get_type_shape() == replacement.get_type_shape()
            }) {
                to_replace.mutate(replacement);
                return Ok(MutationResult::Mutated);
            }
        }

        Ok(MutationResult::Skipped)
    }
}

mutator! {
    /// SKIP:  Removes an input step
    SkipMutator,
    fn mutate(
        &mut self,
        state: &mut S,
        trace: &mut Trace,
        _stage_idx: i32,
    ) -> Result<MutationResult, Error> {
        let steps = &mut trace.steps;
        let length = steps.len();
        if length == 0 {
            return Ok(MutationResult::Skipped);
        }
        let remove_index = state.rand_mut().between(0, (length - 1) as u64) as usize;
        steps.remove(remove_index);
        Ok(MutationResult::Mutated)
    }
}

mutator! {
    /// REPEAT: Repeats an input which is already part of the trace
    RepeatMutator,
    fn mutate(
        &mut self,
        state: &mut S,
        trace: &mut Trace,
        _stage_idx: i32,
    ) -> Result<MutationResult, Error> {
        let steps = &trace.steps;
        let length = steps.len();
        if length == 0 {
            return Ok(MutationResult::Skipped);
        }
        let insert_index = state.rand_mut().between(0, length as u64) as usize;
        let step = state.rand_mut().choose(steps).clone();
        (&mut trace.steps).insert(insert_index, step);
        Ok(MutationResult::Mutated)
    }
}
