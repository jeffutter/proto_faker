use rand::RngCore;
use rand_distr::{Distribution, LogNormal, Normal, Pareto};

pub struct ParetoRng<R: RngCore> {
    inner: R,
    pareto: Pareto<f64>,
}

impl<R: RngCore> ParetoRng<R> {
    pub fn new(inner: R, scale: f64, shape: f64) -> Self {
        let pareto = Pareto::new(scale, shape).unwrap();
        Self { inner, pareto }
    }

    fn sample_pareto(&mut self) -> f64 {
        self.pareto.sample(&mut self.inner)
    }
}

impl<R: RngCore> RngCore for ParetoRng<R> {
    fn next_u32(&mut self) -> u32 {
        let sample = self.sample_pareto();
        let normalized = sample / (sample + 1.0); // Normalize into (0,1)
        (normalized * u32::MAX as f64).min(u32::MAX as f64) as u32
    }

    fn next_u64(&mut self) -> u64 {
        let sample = self.sample_pareto();
        let normalized = sample / (sample + 1.0); // Normalize into (0,1)
        (normalized * u64::MAX as f64).min(u64::MAX as f64) as u64
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut i = 0;
        while i < dest.len() {
            let rand = self.next_u64();
            let bytes = rand.to_le_bytes();
            let n = (dest.len() - i).min(8);
            dest[i..i + n].copy_from_slice(&bytes[..n]);
            i += n;
        }
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

    fn sample_normal(&mut self) -> f64 {
        self.normal.sample(&mut self.inner)
    }
}

impl<R: RngCore> RngCore for NormalRng<R> {
    fn next_u32(&mut self) -> u32 {
        let sample = self.sample_normal();
        let normalized = 1.0 / (1.0 + (-sample).exp()); // sigmoid(x)
        (normalized * u32::MAX as f64).min(u32::MAX as f64) as u32
    }

    fn next_u64(&mut self) -> u64 {
        let sample = self.sample_normal();
        let normalized = 1.0 / (1.0 + (-sample).exp()); // sigmoid(x)
        (normalized * u64::MAX as f64).min(u64::MAX as f64) as u64
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut i = 0;
        while i < dest.len() {
            let rand = self.next_u64();
            let bytes = rand.to_le_bytes();
            let n = (dest.len() - i).min(8);
            dest[i..i + n].copy_from_slice(&bytes[..n]);
            i += n;
        }
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

    fn sample_lognormal(&mut self) -> f64 {
        self.lognormal.sample(&mut self.inner)
    }
}

impl<R: RngCore> RngCore for LogNormalRng<R> {
    fn next_u32(&mut self) -> u32 {
        let sample = self.sample_lognormal();
        let normalized = sample / (sample + 1.0); // similar normalization as Pareto
        (normalized * u32::MAX as f64).min(u32::MAX as f64) as u32
    }

    fn next_u64(&mut self) -> u64 {
        let sample = self.sample_lognormal();
        let normalized = sample / (sample + 1.0);
        (normalized * u64::MAX as f64).min(u64::MAX as f64) as u64
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut i = 0;
        while i < dest.len() {
            let rand = self.next_u64();
            let bytes = rand.to_le_bytes();
            let n = (dest.len() - i).min(8);
            dest[i..i + n].copy_from_slice(&bytes[..n]);
            i += n;
        }
    }
}

