/// MINIMAL REPRO: LLVM bug on aarch64-linux triggered by u128 arithmetic in borrowing_sub
///
/// This demonstrates that using WideWord (u128) arithmetic in the borrowing_sub
/// implementation causes an infinite loop in release mode on aarch64-linux.
///
/// HANGS: Uses u128 arithmetic (like crypto-bigint)
/// WORKS: Using simple u64::overflowing_sub (see test_hang2.rs)

use rand_core::{RngCore, SeedableRng};
use subtle::{Choice, ConstantTimeEq, ConstantTimeGreater, ConstantTimeLess};

const LIMBS: usize = 5;

// The key function that triggers the bug when used in a rejection sampling loop
#[inline(always)]
const fn borrowing_sub_wideword(lhs: u64, rhs: u64, borrow: u64) -> (u64, u64) {
    let a = lhs as u128;
    let b = rhs as u128;
    let borrow = (borrow >> 63) as u128;  // Extract borrow bit
    let ret = a.wrapping_sub(b + borrow);
    (ret as u64, (ret >> 64) as u64)
}

#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
struct Limb(u64);

impl Limb {
    const ZERO: Self = Self(0);

    #[inline(always)]
    const fn borrowing_sub(self, rhs: Self, borrow: Self) -> (Self, Self) {
        let (res, borrow) = borrowing_sub_wideword(self.0, rhs.0, borrow.0);
        (Limb(res), Limb(borrow))
    }
}

impl From<u64> for Limb {
    fn from(val: u64) -> Self {
        Self(val)
    }
}

impl std::ops::BitAnd for Limb {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

#[derive(Copy, Clone, Debug)]
struct Uint {
    limbs: [Limb; LIMBS],
}

impl Uint {
    const ZERO: Self = Self { limbs: [Limb::ZERO; LIMBS] };
    const BITS: u32 = 64 * LIMBS as u32;

    #[inline(always)]
    const fn borrowing_sub(&self, rhs: &Self, mut borrow: Limb) -> (Self, Limb) {
        let mut limbs = [Limb::ZERO; LIMBS];
        let mut i = 0;
        while i < LIMBS {
            let (w, b) = self.limbs[i].borrowing_sub(rhs.limbs[i], borrow);
            limbs[i] = w;
            borrow = b;
            i += 1;
        }
        (Self { limbs }, borrow)
    }

    #[inline]
    fn lt(lhs: &Self, rhs: &Self) -> Choice {
        let (_res, borrow) = lhs.borrowing_sub(rhs, Limb::ZERO);
        Choice::from((borrow.0 != 0) as u8)
    }
}

impl AsMut<[Limb]> for Uint {
    fn as_mut(&mut self) -> &mut [Limb] {
        &mut self.limbs
    }
}

impl AsRef<[Limb]> for Uint {
    fn as_ref(&self) -> &[Limb] {
        &self.limbs
    }
}

impl ConstantTimeEq for Uint {
    fn ct_eq(&self, other: &Self) -> Choice {
        let mut acc = 0;
        for i in 0..LIMBS {
            acc |= self.limbs[i].0 ^ other.limbs[i].0;
        }
        Choice::from(((acc | acc.wrapping_neg()) >> 63) as u8 ^ 1)
    }
}

impl ConstantTimeGreater for Uint {
    fn ct_gt(&self, other: &Self) -> Choice {
        other.ct_lt(self)
    }
}

impl ConstantTimeLess for Uint {
    #[inline]
    fn ct_lt(&self, other: &Self) -> Choice {
        Uint::lt(self, other)
    }
}

#[derive(Clone, Copy)]
struct NonZero<T>(T);

impl<T> NonZero<T> {
    fn new(val: T) -> Option<Self> {
        Some(Self(val))
    }
}

impl NonZero<Uint> {
    fn bits_vartime(&self) -> u32 {
        Uint::BITS
    }
}

impl<T> AsRef<T> for NonZero<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

fn my_random_mod(rng: &mut impl RngCore, modulus: &NonZero<Uint>) -> Uint {
    let mut n = Uint::ZERO;
    let _ = random_mod_core(rng, &mut n, modulus, modulus.bits_vartime());
    n
}

fn random_mod_core<T, R: RngCore + ?Sized>(
    rng: &mut R,
    n: &mut T,
    modulus: &NonZero<T>,
    n_bits: u32,
) -> Result<(), std::io::Error>
where
    T: AsMut<[Limb]> + AsRef<[Limb]> + ConstantTimeLess,
{
    for _ in 0..u32::MAX {
        random_bits_core(rng, n.as_mut(), n_bits)?;

        if n.ct_lt(modulus.as_ref()).into() {
            return Ok(());
        }
    }
    panic!("got really unlucky");
}

fn random_bits_core<R: RngCore + ?Sized>(
    rng: &mut R,
    zeroed_limbs: &mut [Limb],
    bit_length: u32,
) -> Result<(), std::io::Error> {
    if bit_length == 0 {
        return Ok(());
    }

    let buffer: u64 = 0;
    let mut buffer = buffer.to_be_bytes();

    let nonzero_limbs = bit_length.div_ceil(64) as usize;
    let partial_limb = bit_length % 64;
    let mask = u64::MAX >> ((64 - partial_limb) % 64);

    for i in 0..nonzero_limbs - 1 {
        rng.fill_bytes(&mut buffer);
        zeroed_limbs[i] = Limb::from(u64::from_le_bytes(buffer));
    }

    let slice = if partial_limb > 0 && partial_limb <= 32 {
        &mut buffer[0..4]
    } else {
        buffer.as_mut_slice()
    };
    rng.fill_bytes(slice);
    zeroed_limbs[nonzero_limbs - 1] = Limb::from(u64::from_le_bytes(buffer)) & Limb::from(mask);

    Ok(())
}

fn random_nonzero_limb(rng: &mut impl RngCore) -> NonZero<Limb> {
    let mut buf = [0u8; 8];
    rng.fill_bytes(&mut buf);
    let val = u64::from_le_bytes(buf);
    NonZero::new(Limb(val | 1)).unwrap()
}

fn main() {
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
    let special = random_nonzero_limb(&mut rng);

    let mut p_val = Uint::ZERO;
    p_val.limbs[0] = Limb(0u64.wrapping_sub(special.0.0));
    for i in 1..LIMBS {
        p_val.limbs[i] = Limb(u64::MAX);
    }
    let p = NonZero::new(p_val).unwrap();

    // HANGS in release mode on aarch64-linux due to LLVM bug with u128 arithmetic
    let a = my_random_mod(&mut rng, &p);
    println!("Hello, {a:?}");
}
