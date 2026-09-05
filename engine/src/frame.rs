//! Assembles the incoming sample stream into fixed-length frames (20ms).
//! At most one partial frame of audio is ever held in memory.

pub struct Framer {
    frame_len: usize,
    buf: Vec<f32>,
}

impl Framer {
    pub fn new(frame_len: usize) -> Self {
        Self {
            frame_len,
            buf: Vec::with_capacity(frame_len),
        }
    }

    /// Feed samples in; get back every completed frame. Leftover samples
    /// (less than one frame) stay buffered for the next call.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        self.buf.extend_from_slice(samples);
        let mut frames = Vec::new();
        while self.buf.len() >= self.frame_len {
            frames.push(self.buf.drain(..self.frame_len).collect());
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_fixed_frames_across_chunks() {
        let mut framer = Framer::new(4);
        assert!(framer.push(&[1.0, 2.0]).is_empty());
        let frames = framer.push(&[3.0, 4.0, 5.0]);
        assert_eq!(frames, vec![vec![1.0, 2.0, 3.0, 4.0]]);
        let frames = framer.push(&[6.0, 7.0, 8.0, 9.0, 10.0]);
        assert_eq!(frames, vec![vec![5.0, 6.0, 7.0, 8.0]]);
    }
}
