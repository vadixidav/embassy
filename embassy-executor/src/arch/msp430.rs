use core::arch::asm;
use core::marker::PhantomData;
use core::ptr;

use atomic_polyfill::{AtomicBool, Ordering};

use super::{raw, Spawner};

/// Global atomic to keep track of if there is work to do
static SIGNAL_WORK_THREAD_MODE: AtomicBool = AtomicBool::new(false);

/// RISCV32 Executor
pub struct Executor {
    inner: raw::Executor,
    not_send: PhantomData<*mut ()>,
}

impl Executor {
    /// Create a new Executor.
    pub fn new() -> Self {
        Self {
            // use Signal_Work_Thread_Mode as substitute for local interrupt register
            inner: raw::Executor::new(
                |_| {
                    SIGNAL_WORK_THREAD_MODE.store(true, Ordering::SeqCst);
                },
                ptr::null_mut(),
            ),
            not_send: PhantomData,
        }
    }

    /// Run the executor.
    ///
    /// The `init` closure is called with a [`Spawner`] that spawns tasks on
    /// this executor. Use it to spawn the initial task(s). After `init` returns,
    /// the executor starts running the tasks.
    ///
    /// To spawn more tasks later, you may keep copies of the [`Spawner`] (it is `Copy`),
    /// for example by passing it as an argument to the initial tasks.
    ///
    /// This function requires `&'static mut self`. This means you have to store the
    /// Executor instance in a place where it'll live forever and grants you mutable
    /// access. There's a few ways to do this:
    ///
    /// - a [StaticCell](https://docs.rs/static_cell/latest/static_cell/) (safe)
    /// - a `static mut` (unsafe)
    /// - a local variable in a function you know never returns (like `fn main() -> !`), upgrading its lifetime with `transmute`. (unsafe)
    ///
    /// This function never returns.
    pub fn run(&'static mut self, init: impl FnOnce(Spawner)) -> ! {
        init(self.inner.spawner());

        loop {
            unsafe {
                self.inner.poll();
                // we do not care about race conditions between the load and store operations, interrupts
                //will only set this value to true.
                critical_section::with(|_| {
                    // if there is work to do, loop back to polling
                    // TODO can we relax this?
                    if SIGNAL_WORK_THREAD_MODE.load(Ordering::SeqCst) {
                        SIGNAL_WORK_THREAD_MODE.store(false, Ordering::SeqCst);
                    }
                    // if not, wait for interrupt
                    else {
                        // TODO: This stops the CPU, but with Rust it isn't yet clear how to clear
                        // the CPUOFF bit in the stack's status register before its popped. Likely
                        // a macro needs to be added to msp430-rt to generate a wrapper that
                        // ensures the SP on the stack is corrected by calling a generated function.
                        // asm!("bis #16, R2", options(nomem, nostack, preserves_flags));
                        asm!("nop", options(nomem, nostack, preserves_flags));
                    }
                });
                // if an interrupt occurred while waiting, it will be serviced here
            }
        }
    }
}
