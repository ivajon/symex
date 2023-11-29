#![no_std]
#![no_main]
//! Examples for how enums are handled.
//!
//! If `symbolic` is just called on an enum `check` shows what happens.
//!
//! ```shell
//! cargo symex --example enum --function check
//! ```
//!
//! This will trigger an `UnreachableInstruction` error, as the enum should be exhaustive and
//! `symbolic` will allow for invalid variants.
//!
//! However `check_valid` shows how to handle these cases.
//!
//! ```shell
//! cargo symex --example enum --function check_valid
//! ```
//!
//! After marking the enum as symbolic,
//! the helper function `is_valid` when derived will suppress the invalid variant and only allow
//! the valid variants.
#![allow(dead_code)]
use panic_halt as _;
use rp2040_hal::entry;
use symex_lib::{symbolic, valid, Validate};

#[derive(Validate)]
enum E {
    A,
    B(u8),
    C(u16),
}

#[inline(never)]
#[no_mangle]
fn check() -> bool {
    let mut e = E::A;

    // This will mark everything as symbolic, including the variant (`A`, `B`, or `C`).
    symbolic(&mut e);

    match e {
        E::A | E::B(_) => true,
        E::C(_) => false,
    }
}

#[inline(never)]
#[no_mangle]
fn check_valid() -> bool {
    let mut e = E::A;

    // This will mark everything as symbolic, including the variant (`A`, `B`, or `C`).
    symbolic(&mut e);

    // But this will suppress the invalid variants, so for the sake of the analysis `e` can only be
    // `A`, `B`, or `C`.
    valid(&e);

    match e {
        E::A | E::B(_) => true,
        E::C(_) => false,
    }
}

#[entry]
fn main() -> ! {
    let n0 = check();
    let n1 = check_valid();

    unsafe {
        let _ = core::ptr::read_volatile(&n0);
        let _ = core::ptr::read_volatile(&n1);
    }

    loop {}
}
