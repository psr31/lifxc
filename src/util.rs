use anyhow::{anyhow, Result};
use std::{
    future::Future,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

pub enum Timeout<T> {
    Resolved(T),
    TimedOut,
}

impl<T> Timeout<T> {
    pub fn expect_resolved(self) -> Result<T> {
        match self {
            Self::Resolved(v) => Ok(v),
            Self::TimedOut => Err(anyhow!("Operation timed out.")),
        }
    }
}

pub async fn timeout<F: Future>(fut: F, ms: u64) -> Timeout<F::Output> {
    match tokio::time::timeout(Duration::from_millis(ms), fut).await {
        Ok(v) => Timeout::Resolved(v),
        Err(_) => Timeout::TimedOut,
    }
}

pub fn parse_address(raw: &str) -> Option<SocketAddr> {
    match raw.parse::<SocketAddr>() {
        Ok(addr) => Some(addr),
        Err(_) => match raw.parse::<IpAddr>() {
            Ok(addr) => Some(SocketAddr::new(addr, crate::LIFX_PORT)),
            Err(_) => None,
        },
    }
}
