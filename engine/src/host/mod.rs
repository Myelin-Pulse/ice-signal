//! Host/testbench side: everything that stays on the computer (later, the
//! phone) when the core moves into the FPGA. Floats, allocation, and OS APIs
//! are all fine here — none of this ports.

pub mod audio;
pub mod events;
pub mod meter;
pub mod resample;
