use crypto_bigint::{NonZero, RandomMod, Uint};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

fn main() {
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    let special = rng.next_u64();
    let n = NonZero::new(Uint::<5>::ZERO.wrapping_sub(&Uint::from(special))).unwrap();
    let a = Uint::random_mod(&mut rng, &n);  // XXX HANGS
    println!("Hello, {a:?}");
}
