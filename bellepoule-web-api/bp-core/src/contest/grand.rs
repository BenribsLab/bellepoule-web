//! Générateur pseudo-aléatoire de GLib (`GRand`, Mersenne Twister MT19937).
//!
//! Le C++ s'en sert pour les graines des phases (`Schedule::PlugStage`) et
//! pour départager aléatoirement mais de façon reproductible deux joueurs
//! (`Player::RandomCompare`). On reproduit exactement l'algorithme
//! (`g_rand_new_with_seed_array` = `init_by_array`, `g_rand_int` =
//! `genrand_int32`) pour obtenir les mêmes tirages d'une exécution à l'autre.

use std::cmp::Ordering;

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

pub struct GRand {
    mt: [u32; N],
    mti: usize,
}

impl GRand {
    /// `g_rand_new_with_seed` (`init_genrand`).
    pub fn with_seed(seed: u32) -> Self {
        let mut mt = [0u32; N];
        mt[0] = seed;
        for i in 1..N {
            mt[i] = 1_812_433_253u32.wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_add(i as u32);
        }
        GRand { mt, mti: N }
    }

    /// `g_rand_new_with_seed_array` (`init_by_array`).
    pub fn with_seed_array(seed: &[u32]) -> Self {
        let mut r = GRand::with_seed(19_650_218);
        let key_length = seed.len().max(1);
        let mut i = 1usize;
        let mut j = 0usize;
        let mut k = N.max(key_length);
        while k > 0 {
            let prev = r.mt[i - 1];
            let s = seed.get(j).copied().unwrap_or(0);
            r.mt[i] = (r.mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1_664_525)).wrapping_add(s).wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                r.mt[0] = r.mt[N - 1];
                i = 1;
            }
            if j >= key_length {
                j = 0;
            }
            k -= 1;
        }
        k = N - 1;
        while k > 0 {
            let prev = r.mt[i - 1];
            r.mt[i] = (r.mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1_566_083_941)).wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                r.mt[0] = r.mt[N - 1];
                i = 1;
            }
            k -= 1;
        }
        r.mt[0] = 0x8000_0000;
        r.mti = N;
        r
    }

    /// `g_rand_int` (`genrand_int32`).
    pub fn next_u32(&mut self) -> u32 {
        if self.mti >= N {
            for kk in 0..N {
                let y = (self.mt[kk] & UPPER_MASK) | (self.mt[(kk + 1) % N] & LOWER_MASK);
                let mag = if y & 1 != 0 { MATRIX_A } else { 0 };
                self.mt[kk] = self.mt[(kk + M) % N] ^ (y >> 1) ^ mag;
            }
            self.mti = 0;
        }
        let mut y = self.mt[self.mti];
        self.mti += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }
}

/// `Player::RandomCompare` : toujours le même résultat pour deux joueurs et
/// une graine donnés.
pub fn random_compare(ref_a: u32, ref_b: u32, rand_seed: u32) -> Ordering {
    let seed = [ref_a.min(ref_b), ref_a.max(ref_b), rand_seed];
    let result = GRand::with_seed_array(&seed).next_u32() as i32;
    let result = if ref_a > ref_b { result.wrapping_neg() } else { result };
    result.cmp(&0)
}

/// Graine d'une phase (`Schedule::PlugStage`) : `g_rand_int` sur la graine
/// `{id de la phase, graine de la phase précédente}`.
pub fn stage_rand_seed(stage_id: u32, previous_seed: u32) -> u32 {
    GRand::with_seed_array(&[stage_id, previous_seed]).next_u32()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_values() {
        // Valeurs de référence de MT19937 (init_by_array {0x123,0x234,0x345,0x456}).
        let mut r = GRand::with_seed_array(&[0x123, 0x234, 0x345, 0x456]);
        assert_eq!(r.next_u32(), 1_067_595_299);
        assert_eq!(r.next_u32(), 955_945_823);
        // init_genrand(5489)
        let mut r = GRand::with_seed(5489);
        assert_eq!(r.next_u32(), 3_499_211_612);
    }

    #[test]
    fn random_compare_is_antisymmetric() {
        assert_eq!(random_compare(3, 7, 42), random_compare(7, 3, 42).reverse());
    }
}
