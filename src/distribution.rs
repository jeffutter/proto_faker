use rand::RngCore;
use rand_distr::{Distribution, LogNormal, Normal, Pareto};

/// A random generator that produces biased samples normalized into [0, 1).
pub trait BiasedRng: RngCore {
    /// Sample a floating-point number in [0, 1) with the bias of the underlying distribution.
    fn sample01(&mut self) -> f64;
    /// Full sample from the underling Rng
    fn _sample(&mut self) -> f64;
}

pub struct ParetoRng<R: RngCore> {
    inner: R,
    pareto: Pareto<f64>,
}

impl<R: RngCore> ParetoRng<R> {
    pub fn new(inner: R, scale: f64, shape: f64) -> Self {
        let pareto = Pareto::new(scale, shape).unwrap();
        Self { inner, pareto }
    }
}

impl<R: RngCore> BiasedRng for ParetoRng<R> {
    fn sample01(&mut self) -> f64 {
        let sample = self._sample();
        sample / (sample + 1.0)
    }

    fn _sample(&mut self) -> f64 {
        self.pareto.sample(&mut self.inner)
    }
}

pub struct NormalRng<R: RngCore> {
    inner: R,
    normal: Normal<f64>,
}

impl<R: RngCore> NormalRng<R> {
    pub fn new(inner: R, mean: f64, std_dev: f64) -> Self {
        let normal = Normal::new(mean, std_dev).unwrap();
        Self { inner, normal }
    }
}

impl<R: RngCore> BiasedRng for NormalRng<R> {
    fn sample01(&mut self) -> f64 {
        let sample = self._sample();
        1.0 / (1.0 + (-sample).exp()) // sigmoid(x)
    }

    fn _sample(&mut self) -> f64 {
        self.normal.sample(&mut self.inner)
    }
}

pub struct LogNormalRng<R: RngCore> {
    inner: R,
    lognormal: LogNormal<f64>,
}

impl<R: RngCore> LogNormalRng<R> {
    pub fn new(inner: R, mean: f64, std_dev: f64) -> Self {
        let lognormal = LogNormal::new(mean, std_dev).unwrap();
        Self { inner, lognormal }
    }
}

impl<R: RngCore> BiasedRng for LogNormalRng<R> {
    fn sample01(&mut self) -> f64 {
        let sample = self._sample();
        sample / (sample + 1.0)
    }

    fn _sample(&mut self) -> f64 {
        self.lognormal.sample(&mut self.inner)
    }
}

/// Fill `dest` using repeated calls to `next_u64`.
pub fn fill_bytes_via_next_u64<R: RngCore>(rng: &mut R, dest: &mut [u8]) {
    let mut i = 0;
    while i < dest.len() {
        let rand = rng.next_u64();
        let bytes = rand.to_le_bytes();
        let n = (dest.len() - i).min(8);
        dest[i..i + n].copy_from_slice(&bytes[..n]);
        i += n;
    }
}

macro_rules! impl_biased_rng {
    ($ty:ty) => {
        impl<R: RngCore> RngCore for $ty {
            fn next_u32(&mut self) -> u32 {
                (self.sample01() * (u32::MAX as f64)) as u32
            }
            fn next_u64(&mut self) -> u64 {
                (self.sample01() * (u64::MAX as f64)) as u64
            }
            fn fill_bytes(&mut self, dest: &mut [u8]) {
                fill_bytes_via_next_u64(self, dest);
            }
        }
    };
}

impl_biased_rng!(ParetoRng<R>);
impl_biased_rng!(NormalRng<R>);
impl_biased_rng!(LogNormalRng<R>);
