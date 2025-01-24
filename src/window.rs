pub struct Window {
    size: u16,
    pub start: u16,
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

    pub fn update(&mut self, ack: u16) -> i64{
        let last_next_send = self.next_send;
        let last_start = self.start;
        if ack.wrapping_sub(self.start) < self.size {
            /* ack 落在窗口内，向前滑动窗口 */
            self.start = ack.wrapping_add(1);
            self.end = self.start.wrapping_add(self.size);
            self.next_send = self.start;
        } else {
            /* 收到已经确认过的 ack，说明有丢包，重传窗口*/
            self.next_send = self.start;
        }
        /* 返回更新窗口后，下次发送序号相较之前的偏移，用于确定下次文件读取位置 */
        /* 以更新前窗口起始位置为参照可以判断到底谁大 */
        let last = last_next_send.wrapping_sub(last_start) as i64;
        let cur = self.next_send.wrapping_sub(last_start) as i64;
        cur - last
    }
}

impl Iterator for Window {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_send == self.end {
            None
        } else {
            let cur = self.next_send;
            self.next_send = self.next_send.wrapping_add(1);
            Some(cur)
        }
    }
}
