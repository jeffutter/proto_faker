use rand::RngCore;
use rand_distr::{Distribution, LogNormal, Normal, Pareto};

/// A wrapper Rng that outputs Pareto-distributed floats
pub struct ParetoRng<R: RngCore> {
    inner: R,
    scale: f64,
    shape: f64,
}

impl<R: RngCore> ParetoRng<R> {
    pub fn new(inner: R, scale: f64, shape: f64) -> Self {
        Self {
            inner,
            scale,
            shape,
        }
    }
}

impl<R: RngCore> RngCore for ParetoRng<R> {
    fn next_u32(&mut self) -> u32 {
        let sample = self.sample_pareto();
        (sample * (u32::MAX as f64)).min(u32::MAX as f64) as u32
    }

    fn next_u64(&mut self) -> u64 {
        let sample = self.sample_pareto();
        (sample * (u64::MAX as f64)).min(u64::MAX as f64) as u64
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut idx = 0;
        while idx < dest.len() {
            let rand_val = self.next_u64();
            let bytes = rand_val.to_le_bytes();
            let chunk_size = (dest.len() - idx).min(8);
            dest[idx..idx + chunk_size].copy_from_slice(&bytes[..chunk_size]);
            idx += chunk_size;
        }
    }
}

impl<R: RngCore> ParetoRng<R> {
    fn sample_pareto(&mut self) -> f64 {
        let pareto = Pareto::new(self.scale, self.shape).unwrap();
        pareto.sample(&mut self.inner)
    }
}

pub struct NormalRng<R: RngCore> {
    inner: R,
    scale: f64,
    shape: f64,
}

impl<R: RngCore> NormalRng<R> {
    pub fn new(inner: R, scale: f64, shape: f64) -> Self {
        Self {
            inner,
            scale,
            shape,
        }
    }
}

impl<R: RngCore> RngCore for NormalRng<R> {
    fn next_u32(&mut self) -> u32 {
        let sample = self.sample_normal();
        (sample * (u32::MAX as f64)).min(u32::MAX as f64) as u32
    }

    fn next_u64(&mut self) -> u64 {
        let sample = self.sample_normal();
        (sample * (u64::MAX as f64)).min(u64::MAX as f64) as u64
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut idx = 0;
        while idx < dest.len() {
            let rand_val = self.next_u64();
            let bytes = rand_val.to_le_bytes();
            let chunk_size = (dest.len() - idx).min(8);
            dest[idx..idx + chunk_size].copy_from_slice(&bytes[..chunk_size]);
            idx += chunk_size;
        }
    }
}

impl<R: RngCore> NormalRng<R> {
    fn sample_normal(&mut self) -> f64 {
        let normal = Normal::new(self.scale, self.shape).unwrap();
        normal.sample(&mut self.inner)
    }
}

pub struct LogNormalRng<R: RngCore> {
    inner: R,
    scale: f64,
    shape: f64,
}

impl<R: RngCore> LogNormalRng<R> {
    pub fn new(inner: R, scale: f64, shape: f64) -> Self {
        Self {
            inner,
            scale,
            shape,
        }
    }
}

impl<R: RngCore> RngCore for LogNormalRng<R> {
    fn next_u32(&mut self) -> u32 {
        let sample = self.sample_log_normal();
        (sample * (u32::MAX as f64)).min(u32::MAX as f64) as u32
    }

    fn next_u64(&mut self) -> u64 {
        let sample = self.sample_log_normal();
        (sample * (u64::MAX as f64)).min(u64::MAX as f64) as u64
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut idx = 0;
        while idx < dest.len() {
            let rand_val = self.next_u64();
            let bytes = rand_val.to_le_bytes();
            let chunk_size = (dest.len() - idx).min(8);
            dest[idx..idx + chunk_size].copy_from_slice(&bytes[..chunk_size]);
            idx += chunk_size;
        }
    }
}

impl<R: RngCore> LogNormalRng<R> {
    fn sample_log_normal(&mut self) -> f64 {
        let log_normal = LogNormal::new(self.scale, self.shape).unwrap();
        log_normal.sample(&mut self.inner)
    }
}

