pub struct Window {
    size: u16,
    start: u16,
    end: u16,
    pub next_send: u16,
}

impl Window {
    pub fn new(size: u16) -> Self {
        Self {
            size: size,
            start: 1,
            end: size.wrapping_add(1),
            next_send: 1,
        }
    }

    pub fn update(&mut self, ack: u16) {
        if ack.wrapping_sub(self.start) < self.size {
            self.start = ack.wrapping_add(1);
            self.next_send = self.start;
            self.end = self.start.wrapping_add(self.size)
        }
    }
}

impl Iterator for Window {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_send == self.end {
            None
        } else {
            let next = Some(self.next_send);
            self.next_send = self.next_send.wrapping_add(1);
            next
        }
    }
}
