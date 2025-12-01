use crypto_bigint::{NonZero, RandomBits, Uint};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use subtle::ConstantTimeLess;

fn bad_random_mod<R>(rng: &mut R, n: &Uint<5>) -> Uint<5>
where
    R: RngCore,
{
    let n_bits = n.bits_vartime();
    loop {
        let x = Uint::random_bits(rng, n_bits);
        if x.ct_lt(n).into() {
            return x;
        }
    }
}

fn main() {
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    let special = rng.next_u64();
    let n = NonZero::new(Uint::<5>::ZERO.wrapping_sub(&Uint::from(special))).unwrap();

    let a = bad_random_mod(&mut rng, &n);

    println!("Hello, {a:?}");
}
