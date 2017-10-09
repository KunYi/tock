//! Virtualizes the 802.15.4 sending interface to multiple clients.

// System call interface for sending and receiving 802.15.4 packets.
//
// Author: Philip Levis
// Date: Jan 12 2017
//

use core::cell::Cell;
use kernel::ReturnCode;
use kernel::common::virtualizer::{QueuedCall, CallQueue, Dequeued};
use kernel::hil::radio;
use ieee802154::mac;
use net::ieee802154::*;

pub struct RadioMux<'a, R: radio::Radio + 'a> {
    mac: &'a mac::MacDevice<'a, R>,
    busy: Cell<bool>,
    queue: CallQueue<'a>,
}

pub struct VirtualRadioDevice<'a, R: radio::Radio + 'a> {
    tx_buffer: Cell<Option<mac::Frame>>,
    queued_call: QueuedCall<'a>,
    mux: &'a RadioMux<'a, R>,
    client: Cell<Option<&'a mac::TxClient>>,
}

pub trait VirtualTransmit<'a> {
    fn transmit(&'a self, frame: mac::Frame) -> (ReturnCode, Option<&'static mut [u8]>);
}

impl<'a, R: radio::Radio> RadioMux<'a, R> {
    pub fn new(mac: &'a mac::MacDevice<'a, R>) -> RadioMux<'a, R> {
        RadioMux {
            mac: mac,
            busy: Cell::new(false),
            queue: CallQueue::new(),
        }
    }

    pub fn busy(&self) -> bool {
        self.busy.get()
    }

    pub fn next(&self) -> ReturnCode {
        if !self.busy() {
            self.busy.set(true);
            self.queue.dequeue_and_trigger();
        }
        ReturnCode::SUCCESS
    }

    pub fn clear_busy(&self) {
        self.busy.set(false);
    }

    pub fn set_transmit_client(&self, client: &'a mac::TxClient) {
        self.mac.set_transmit_client(client);
    }
}

impl<'a, R: radio::Radio> VirtualRadioDevice<'a, R> {
    pub fn new(mux: &'a RadioMux<'a, R>) -> VirtualRadioDevice<'a, R> {
        VirtualRadioDevice {
            tx_buffer: Cell::new(None),
            queued_call: QueuedCall::new(&mux.queue),
            mux: mux,
            client: Cell::new(None),
        }
    }

    pub fn init(&'a self, client: &'a mac::TxClient) {
        self.client.set(Some(client));
        self.queued_call.set_callback(self);
    }
}

impl<'a, R: radio::Radio> mac::TxClient for VirtualRadioDevice<'a, R> {
    fn send_done(&self, buf: &'static mut [u8], acked: bool, result: ReturnCode) {
        self.client.get().map(move |c| c.send_done(buf, acked, result));
        self.mux.clear_busy();
        self.mux.next();
    }
}

impl<'a, R: radio::Radio> Dequeued<'a> for VirtualRadioDevice<'a, R> {
    fn id(&'a self) -> u32 {
        0
    }
    fn dequeued(&'a self) {
        self.mux.set_transmit_client(self);
        //self.mac.transmit(self.tx_buffer.unwrap())
    }
}

impl<'a, R: radio::Radio> mac::Mac<'a> for VirtualRadioDevice<'a, R> {
    fn get_address(&self) -> u16 {
        self.mux.mac.get_address()
    }
    fn get_address_long(&self) -> [u8; 8] {
        self.mux.mac.get_address_long()
    }
    fn get_pan(&self) -> u16 {
        self.mux.mac.get_pan()
    }
    fn get_channel(&self) -> u8 {
        self.mux.mac.get_channel()
    }
    fn get_tx_power(&self) -> i8 {
        self.mux.mac.get_tx_power()
    }
    fn set_address(&self, addr: u16) {
        self.mux.mac.set_address(addr);
    }
    fn set_address_long(&self, addr: [u8; 8]) {
        self.mux.mac.set_address_long(addr);
    }
    fn set_pan(&self, id: u16) {
        self.mux.mac.set_pan(id);
    }
    fn set_channel(&self, chan: u8) -> ReturnCode {
        self.mux.mac.set_channel(chan)
    }
    fn set_tx_power(&self, power: i8) -> ReturnCode {
        self.mux.mac.set_tx_power(power)
    }

    fn config_commit(&self) {
        self.mux.mac.config_commit()
    }
    fn is_on(&self) -> bool {
        self.mux.mac.is_on()
    }
    fn prepare_data_frame(&self,
                          buf: &'static mut [u8],
                          dst_pan: PanID,
                          dst_addr: MacAddress,
                          src_pan: PanID,
                          src_addr: MacAddress,
                          security_needed: Option<(SecurityLevel, KeyId)>)
                          -> Result<mac::Frame, &'static mut [u8]> {
        self.mux.mac.prepare_data_frame(buf, dst_pan, dst_addr, src_pan, src_addr, security_needed)
    }

    /// Transmits a frame that has been prepared by the above process. If the
    /// transmission process fails, the buffer inside the frame is returned so
    /// that it can be re-used.
    fn transmit(&self, frame: mac::Frame) -> (ReturnCode, Option<&'static mut [u8]>) {
        return (ReturnCode::ENOSUPPORT, Some(frame.into_buf()));
    }
    fn set_transmit_client(&self, client: &'a mac::TxClient) {
        self.mux.mac.set_transmit_client(client);        
    }

    fn set_receive_client(&self, client: &'a mac::RxClient) {
        self.mux.mac.set_receive_client(client);        
    }
}

impl<'a, R: radio::Radio> VirtualTransmit<'a> for VirtualRadioDevice<'a, R> {
    fn transmit(&'a self, frame: mac::Frame) -> (ReturnCode, Option<&'static mut [u8]>) {
        if self.queued_call.insert() {
            self.tx_buffer.set(Some(frame));
            self.mux.next(); // If already busy will do nothing
            return (ReturnCode::SUCCESS, None);
        } else {
            return (ReturnCode::EBUSY, Some(frame.into_buf()));
        }
    }
}