//! Reactive Asynchronous Mutations (use_mutation).

use std::rc::Rc;
use strata_dom::reactive::Signal;

/// Reactive result handle returned by `use_mutation`.
#[derive(Clone)]
pub struct MutationResult<TData: Clone + 'static, TError: Clone + 'static, TVariables: Clone + 'static> {
    pub data: Signal<Option<TData>>,
    pub error: Signal<Option<TError>>,
    pub is_pending: Signal<bool>,
    pub is_success: Signal<bool>,
    mutate_fn: Rc<dyn Fn(TVariables)>,
}

impl<TData: Clone + 'static, TError: Clone + 'static, TVariables: Clone + 'static>
    MutationResult<TData, TError, TVariables>
{
    /// Trigger execution of the mutation with variables.
    pub fn mutate(&self, vars: TVariables) {
        (self.mutate_fn)(vars);
    }
}

/// Hook to manage asynchronous mutations.
pub fn use_mutation<TData, TError, TVariables, F>(
    mutator: F,
) -> MutationResult<TData, TError, TVariables>
where
    TData: Clone + 'static,
    TError: Clone + 'static,
    TVariables: Clone + 'static,
    F: Fn(TVariables) -> Result<TData, TError> + 'static,
{
    let data_signal = Signal::new(None);
    let error_signal = Signal::new(None);
    let is_pending_signal = Signal::new(false);
    let is_success_signal = Signal::new(false);

    let data_sig = data_signal.clone();
    let err_sig = error_signal.clone();
    let pend_sig = is_pending_signal.clone();
    let succ_sig = is_success_signal.clone();
    let mutator_rc = Rc::new(mutator);

    let mutate_fn = Rc::new(move |vars: TVariables| {
        pend_sig.set(true);
        match mutator_rc(vars) {
            Ok(res) => {
                data_sig.set(Some(res));
                err_sig.set(None);
                succ_sig.set(true);
            }
            Err(e) => {
                err_sig.set(Some(e));
                data_sig.set(None);
                succ_sig.set(false);
            }
        }
        pend_sig.set(false);
    });

    MutationResult {
        data: data_signal,
        error: error_signal,
        is_pending: is_pending_signal,
        is_success: is_success_signal,
        mutate_fn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_use_mutation_lifecycle() {
        let mutation = use_mutation(|username: String| {
            if username.is_empty() {
                Err("Username cannot be empty")
            } else {
                Ok(format!("Created user {username}"))
            }
        });

        assert!(!mutation.is_pending.get());
        assert!(!mutation.is_success.get());

        // Error mutation
        mutation.mutate("".into());
        assert_eq!(mutation.error.get().unwrap(), "Username cannot be empty");
        assert!(!mutation.is_success.get());

        // Successful mutation
        mutation.mutate("alex".into());
        assert!(mutation.is_success.get());
        assert_eq!(mutation.data.get().unwrap(), "Created user alex");
        assert!(mutation.error.get().is_none());
    }
}
