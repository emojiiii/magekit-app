//! SM3 哈希算法实现
//! SM3 是中国国家密码管理局发布的密码哈希算法

const IV: [u32; 8] = [
    0x7380166f, // 1937774191
    0x4914b2b9, // 1226093241
    0x172442d7, // 388252375
    0xda8a0600, // 3666478592
    0xa96f30bc, // 2842636476
    0x163138aa, // 372324522
    0xe38dee4d, // 3817729613
    0xb0fb0e4e, // 2969243214
];

pub struct SM3 {
    reg: [u32; 8],
    chunk: Vec<u8>,
    size: usize,
}

impl SM3 {
    pub fn new() -> Self {
        Self {
            reg: IV,
            chunk: Vec::new(),
            size: 0,
        }
    }

    pub fn reset(&mut self) {
        self.reg = IV;
        self.chunk.clear();
        self.size = 0;
    }

    pub fn write(&mut self, data: &[u8]) {
        self.size += data.len();
        let mut f = 64 - self.chunk.len();

        if data.len() < f {
            self.chunk.extend_from_slice(data);
        } else {
            self.chunk.extend_from_slice(&data[..f]);

            while self.chunk.len() >= 64 {
                let chunk_to_process = self.chunk[..64].to_vec();
                self.compress(&chunk_to_process);

                if f < data.len() {
                    let end = (f + 64).min(data.len());
                    self.chunk = data[f..end].to_vec();
                } else {
                    self.chunk.clear();
                }
                f += 64;
            }
        }
    }

    fn fill(&mut self) {
        let bit_length = (self.size * 8) as u64;

        // 添加填充位
        let mut padding_pos = self.chunk.len();
        self.chunk.push(0x80);
        padding_pos = (padding_pos + 1) % 64;

        // 如果剩余空间不足8字节，则填充到下一个块
        let mut padding_pos_signed = padding_pos as i32;
        if 64 - padding_pos < 8 {
            padding_pos_signed -= 64;
        }

        // 填充0直到剩余8字节用于存储长度
        while padding_pos_signed < 56 {
            self.chunk.push(0);
            padding_pos_signed += 1;
        }

        // 添加消息长度（高32位）
        let high_bits = (bit_length >> 32) as u32;
        for i in 0..4 {
            self.chunk.push(((high_bits >> (8 * (3 - i))) & 0xFF) as u8);
        }

        // 添加消息长度（低32位）
        for i in 0..4 {
            self.chunk
                .push(((bit_length >> (8 * (3 - i))) & 0xFF) as u8);
        }
    }

    fn compress(&mut self, data: &[u8]) {
        if data.len() < 64 {
            return;
        }

        let mut w = [0u32; 68];
        let mut w_prime = [0u32; 64];

        // 将字节数组转换为字
        for t in 0..16 {
            w[t] = u32::from_be_bytes([
                data[4 * t],
                data[4 * t + 1],
                data[4 * t + 2],
                data[4 * t + 3],
            ]);
        }

        // 消息扩展
        for j in 16..68 {
            let a = w[j - 16] ^ w[j - 9] ^ w[j - 3].rotate_left(15);
            let p1 = a ^ a.rotate_left(15) ^ a.rotate_left(23);
            w[j] = p1 ^ w[j - 13].rotate_left(7) ^ w[j - 6];
        }

        // 计算 W'
        for j in 0..64 {
            w_prime[j] = w[j] ^ w[j + 4];
        }

        // 压缩
        let mut a = self.reg[0];
        let mut b = self.reg[1];
        let mut c = self.reg[2];
        let mut d = self.reg[3];
        let mut e = self.reg[4];
        let mut f = self.reg[5];
        let mut g = self.reg[6];
        let mut h = self.reg[7];

        for j in 0..64 {
            let ss1 = (a
                .rotate_left(12)
                .wrapping_add(e)
                .wrapping_add(get_t_j(j).rotate_left(j as u32)))
            .rotate_left(7);
            let ss2 = ss1 ^ a.rotate_left(12);
            let tt1 = ff_j(j, a, b, c)
                .wrapping_add(d)
                .wrapping_add(ss2)
                .wrapping_add(w_prime[j]);
            let tt2 = gg_j(j, e, f, g)
                .wrapping_add(h)
                .wrapping_add(ss1)
                .wrapping_add(w[j]);

            d = c;
            c = b.rotate_left(9);
            b = a;
            a = tt1;
            h = g;
            g = f.rotate_left(19);
            f = e;
            e = p0(tt2);
        }

        // 更新寄存器
        self.reg[0] ^= a;
        self.reg[1] ^= b;
        self.reg[2] ^= c;
        self.reg[3] ^= d;
        self.reg[4] ^= e;
        self.reg[5] ^= f;
        self.reg[6] ^= g;
        self.reg[7] ^= h;
    }

    pub fn sum(&mut self, data: Option<&[u8]>) -> Vec<u8> {
        if let Some(d) = data {
            self.reset();
            self.write(d);
        }

        self.fill();

        // 分块压缩 - 先克隆数据避免借用冲突
        let chunk = self.chunk.clone();
        let mut pos = 0;
        while pos + 64 <= chunk.len() {
            self.compress(&chunk[pos..pos + 64]);
            pos += 64;
        }

        // 输出结果
        let mut result = Vec::new();
        for val in &self.reg {
            result.extend_from_slice(&val.to_be_bytes());
        }

        self.reset();
        result
    }
}

fn get_t_j(j: usize) -> u32 {
    if j < 16 {
        0x79cc4519 // 2043430169
    } else {
        0x7a879d8a // 2055708042
    }
}

fn ff_j(j: usize, x: u32, y: u32, z: u32) -> u32 {
    if j < 16 {
        x ^ y ^ z
    } else {
        (x & y) | (x & z) | (y & z)
    }
}

fn gg_j(j: usize, x: u32, y: u32, z: u32) -> u32 {
    if j < 16 {
        x ^ y ^ z
    } else {
        (x & y) | (!x & z)
    }
}

fn p0(x: u32) -> u32 {
    x ^ x.rotate_left(9) ^ x.rotate_left(17)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sm3() {
        let mut sm3 = SM3::new();
        let data = b"abc";
        let result = sm3.sum(Some(data));

        // SM3("abc") 的标准结果
        let expected: [u8; 32] = [
            0x66, 0xc7, 0xf0, 0xf4, 0x62, 0xee, 0xed, 0xd9, 0xd1, 0xf2, 0xd4, 0x6b, 0xdc, 0x10,
            0xe4, 0xe2, 0x41, 0x67, 0xc4, 0x87, 0x5c, 0xf2, 0xf7, 0xa2, 0x29, 0x7d, 0xa0, 0x2b,
            0x8f, 0x4b, 0xa8, 0xe0,
        ];
        assert_eq!(result, expected);
    }
}
