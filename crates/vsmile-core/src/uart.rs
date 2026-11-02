//! UART do SPG200. No V.Smile é por aqui que o console conversa com o
//! controlador (joystick), em modo serial. Transmissão e recepção têm atraso
//! proporcional ao baud configurado; ao completar, geram IRQ de UART.

use crate::io::SystemIo;
use crate::irq::Irq;

const CTRL_TX_ENABLE: u16 = 1 << 7;
const CTRL_RX_ENABLE: u16 = 1 << 6;
const CTRL_MODE: u16 = 1 << 5;
const CTRL_TX_IRQ_ENABLE: u16 = 1 << 1;
const CTRL_RX_IRQ_ENABLE: u16 = 1 << 0;

const ST_RX_FULL: u16 = 1 << 7;
const ST_TX_BUSY: u16 = 1 << 6;
const ST_BIT9: u16 = 1 << 5;
const ST_TX_READY: u16 = 1 << 1;
const ST_RX_READY: u16 = 1 << 0;
const STATUS_CLEAR_MASK: u16 = 0x0003;

pub struct Uart {
    control: u16,
    status: u16,
    baud_lo: u8,
    baud_hi: u8,
    tx_buf: u8,
    tx_running: u8,
    rx_buf: u8,
    rx_running: u8,
    tx_counter: i32,
    rx_counter: i32,
}

impl Uart {
    pub fn new() -> Self {
        let mut u = Self {
            control: 0,
            status: 0,
            baud_lo: 0,
            baud_hi: 0,
            tx_buf: 0,
            tx_running: 0,
            rx_buf: 0,
            rx_running: 0,
            tx_counter: 0,
            rx_counter: 0,
        };
        u.reset();
        u
    }

    pub fn reset(&mut self) {
        self.control = 0;
        self.status = ST_BIT9;
        self.baud_lo = 0;
        self.baud_hi = 0;
        self.tx_buf = 0;
        self.tx_running = 0;
        self.rx_buf = 0;
        self.rx_running = 0;
        self.tx_counter = 0;
        self.rx_counter = 0;
    }

    #[inline]
    fn baud_delay(&self) -> i32 {
        let baud = ((self.baud_hi as u32) << 8) | self.baud_lo as u32;
        (16 * (65536 - baud) * if self.control & CTRL_MODE != 0 { 11 } else { 10 }) as i32
    }

    fn update_irq(&self, irq: &mut Irq) {
        let rx_active = self.control & CTRL_RX_IRQ_ENABLE != 0 && self.status & ST_RX_READY != 0;
        let tx_active = self.control & CTRL_TX_IRQ_ENABLE != 0 && self.status & ST_TX_READY != 0;
        irq.set_uart(rx_active || tx_active);
    }

    pub fn run_cycles<Io: SystemIo>(&mut self, cycles: u32, irq: &mut Irq, io: &mut Io) {
        if self.tx_counter != 0 {
            self.tx_counter -= cycles as i32;
            if self.tx_counter <= 0 {
                self.tx_counter = 0;
                self.status |= ST_TX_READY;
                self.status &= !ST_TX_BUSY;
                io.tx_uart(self.tx_running);
                self.update_irq(irq);
            }
        }

        if self.rx_counter != 0 {
            self.rx_counter -= cycles as i32;
            if self.rx_counter <= 0 {
                self.rx_counter = 0;
                self.status |= ST_RX_FULL | ST_RX_READY;
                self.rx_buf = self.rx_running;
                io.rx_uart_done();
                self.update_irq(irq);
            }
        }
    }

    pub fn control(&self) -> u16 {
        self.control
    }
    pub fn set_control(&mut self, value: u16, irq: &mut Irq) {
        let old_tx_enable = self.control & CTRL_TX_ENABLE != 0;
        self.control = value & 0xff;

        if self.control & CTRL_RX_ENABLE == 0 {
            self.rx_buf = 0;
        }
        self.update_irq(irq);

        let tx_enable = self.control & CTRL_TX_ENABLE != 0;
        if tx_enable != old_tx_enable {
            if tx_enable {
                self.status |= ST_TX_READY;
            } else {
                self.status &= !ST_TX_READY;
                self.status &= !ST_TX_BUSY;
                self.tx_counter = 0;
            }
        }
    }

    pub fn status(&self) -> u16 {
        self.status
    }
    pub fn set_status(&mut self, value: u16, irq: &mut Irq) {
        self.status &= !(value & STATUS_CLEAR_MASK);
        self.update_irq(irq);
    }

    pub fn soft_reset(&mut self) {
        // TODO: confirmar o que precisa ser reiniciado.
    }

    pub fn baud_lo(&self) -> u16 {
        self.baud_lo as u16
    }
    pub fn set_baud_lo(&mut self, value: u16) {
        self.baud_lo = (value & 0xff) as u8;
    }
    pub fn baud_hi(&self) -> u16 {
        self.baud_hi as u16
    }
    pub fn set_baud_hi(&mut self, value: u16) {
        self.baud_hi = (value & 0xff) as u8;
    }

    pub fn tx_reg(&self) -> u16 {
        self.tx_buf as u16
    }
    pub fn tx(&mut self, value: u16) {
        self.tx_buf = value as u8;
        if self.control & CTRL_TX_ENABLE != 0 && self.status & ST_TX_BUSY == 0 {
            self.tx_running = value as u8;
            self.status &= !ST_TX_READY;
            self.status |= ST_TX_BUSY;
            self.tx_counter = self.baud_delay();
        }
    }

    /// Leitura de RX (com efeito colateral: limpa rx_full).
    pub fn rx(&mut self) -> u16 {
        self.status &= !ST_RX_FULL;
        self.rx_buf as u16
    }
    /// Leitura de RX sem efeitos (para debug/memory editor).
    pub fn peek_rx(&self) -> u16 {
        self.rx_buf as u16
    }

    /// Início de recepção de um byte vindo do dispositivo externo.
    pub fn rx_start(&mut self, value: u8) {
        if self.rx_counter != 0 {
            return;
        }
        if self.control & CTRL_RX_ENABLE == 0 {
            return;
        }
        self.rx_counter = self.baud_delay();
        self.rx_running = value;
    }
}

impl Default for Uart {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_state!(Uart {
    control, status, baud_lo, baud_hi,
    tx_buf, tx_running, rx_buf, rx_running, tx_counter, rx_counter
});
