use crate::{Message, Response};
use anyhow::{anyhow, Result};
use futures::Stream;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::task::Poll;
use tokio::net::UdpSocket;

const UNEXPECTED_PACKET: &str = "Unexpected packet received from device.";

pub struct LightConnection {
    sock: UdpSocket,
    addr: SocketAddr,
    sequence: u8,
}

impl LightConnection {
    pub async fn new(addr: SocketAddr) -> Result<LightConnection> {
        Ok(LightConnection {
            sock: UdpSocket::bind("0.0.0.0:0").await?,
            addr,
            sequence: 0,
        })
    }

    pub async fn device_stream() -> Result<DeviceStream> {
        let sock = UdpSocket::bind("0.0.0.0:0").await?;
        sock.set_broadcast(true)?;

        // 0x02 - GetService
        sock.send_to(
            &Message::GetService.encode(false, 0, None),
            SocketAddr::from(([255, 255, 255, 255], crate::LIFX_PORT)),
        )
        .await?;

        Ok(DeviceStream {
            sock,
            seen: HashSet::new(),
        })
    }

    pub async fn get_power(&mut self) -> Result<bool> {
        // send request
        self.send_message(Message::GetPower, false).await?;

        // receive response
        let response = self.receive_response().await?;
        if let Some(Message::StatePower(power)) = response.message {
            Ok(power)
        } else {
            Err(anyhow!(UNEXPECTED_PACKET))
        }
    }

    pub async fn set_power(&mut self, power: bool) -> Result<()> {
        self.send_message(Message::SetPower(power), true).await
    }

    pub async fn get_label(&mut self) -> Result<String> {
        self.send_message(Message::GetLabel, false).await?;

        let response = self.receive_response().await?;
        if let Some(Message::StateLabel(label)) = response.message {
            Ok(label)
        } else {
            Err(anyhow!(UNEXPECTED_PACKET))
        }
    }

    pub async fn set_label(&mut self, label: &str) -> Result<()> {
        self.send_message(Message::SetLabel(label.to_string()), true)
            .await
    }

    pub async fn get_state(&mut self) -> Result<(u16, u16, u16, u16, bool, String)> {
        self.send_message(Message::GetColor, false).await?;

        let response = self.receive_response().await?;
        if let Some(Message::LightState(h, s, b, k, power, label)) = response.message {
            Ok((h, s, b, k, power, label))
        } else {
            Err(anyhow!(UNEXPECTED_PACKET))
        }
    }

    pub async fn set_color(
        &mut self,
        hue: u16,
        saturation: u16,
        brightness: u16,
        kelvin: u16,
        duration: u32,
    ) -> Result<()> {
        self.send_message(
            Message::SetColor(hue, saturation, brightness, kelvin, duration),
            true,
        )
        .await
    }

    async fn send_message(&mut self, message: Message, require_ack: bool) -> Result<()> {
        let packet = message.encode(require_ack, self.sequence, None);
        self.sequence = self.sequence.wrapping_add(1);
        self.sock.send_to(&packet, self.addr).await?;

        if require_ack {
            let _response = self.receive_response().await?;
        }

        Ok(())
    }

    async fn receive_response(&self) -> Result<Response> {
        let mut buf = [0u8; 1024];
        self.sock.recv(&mut buf).await?;
        Ok(Response::decode(&buf)?)
    }
}

pub struct DeviceStream {
    sock: UdpSocket,
    seen: HashSet<SocketAddr>,
}

impl Stream for DeviceStream {
    type Item = SocketAddr;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut buf = [0u8; 1024];
        let mut rbuf = tokio::io::ReadBuf::new(&mut buf);

        match self.sock.poll_recv_from(cx, &mut rbuf) {
            Poll::Ready(Ok(addr)) if !self.seen.contains(&addr) => {
                self.seen.insert(addr);
                Poll::Ready(Some(addr))
            }
            Poll::Ready(Ok(_)) => Poll::Pending,
            Poll::Ready(Err(_)) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
