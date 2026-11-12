use anyhow::{bail, Context, Result};
use num_bigint::BigUint;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rc4::{KeyInit, Rc4, StreamCipher};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const DH_P_HEX: &str = "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245E485B576625E7EC6F44C42E9A63A3620FFFFFFFFFFFFFFFF";
const DH_G: u32 = 2;
const CRYPTO_RC4: u32 = 2;

pub struct MseStream {
    pub stream: TcpStream,
    encrypt: Rc4<rc4::consts::U20>,
    decrypt: Rc4<rc4::consts::U20>,
}

impl MseStream {
    pub fn new(stream: TcpStream, encrypt_key: [u8; 20], decrypt_key: [u8; 20]) -> Self {
        Self {
            stream,
            encrypt: Rc4::new(&encrypt_key.into()),
            decrypt: Rc4::new(&decrypt_key.into()),
        }
    }

    pub async fn write_encrypted(&mut self, data: &[u8]) -> Result<()> {
        let mut buf = data.to_vec();
        self.encrypt.apply_keystream(&mut buf);
        self.stream.write_all(&buf).await?;
        self.stream.flush().await?;
        Ok(())
    }

    pub async fn read_decrypted(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = self.stream.read(buf).await?;
        if n > 0 {
            self.decrypt.apply_keystream(&mut buf[..n]);
        }
        Ok(n)
    }
}

fn sha1_hash(data: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let mut out = [0u8; 20];
    out.copy_from_slice(&hasher.finalize());
    out
}

pub async fn mse_handshake_client(mut stream: TcpStream, info_hash: &[u8; 20]) -> Result<MseStream> {
    let p = BigUint::parse_bytes(DH_P_HEX.as_bytes(), 16).context("无效的 DH 素数")?;
    let g = BigUint::from(DH_G);
    let mut rng = StdRng::from_entropy();
    
    // 1. 发送 Ya + PadA
    let mut xa_bytes = [0u8; 20];
    rng.fill(&mut xa_bytes);
    let xa = BigUint::from_bytes_be(&xa_bytes);
    let ya = g.modpow(&xa, &p);
    let mut ya_bytes = ya.to_bytes_be();
    while ya_bytes.len() < 96 { ya_bytes.insert(0, 0); }
    let mut pad_a = vec![0u8; rng.gen_range(0..512)];
    rng.fill(&mut pad_a[..]);
    stream.write_all(&ya_bytes).await?;
    stream.write_all(&pad_a).await?;
    stream.flush().await?;

    // 2. 接收 Yb
    let mut yb_bytes = [0u8; 96];
    stream.read_exact(&mut yb_bytes).await?;
    let yb = BigUint::from_bytes_be(&yb_bytes);

    // 3. 计算共享密钥 S
    let s = yb.modpow(&xa, &p);
    let mut s_bytes = s.to_bytes_be();
    while s_bytes.len() < 96 { s_bytes.insert(0, 0); }

    // 4. 计算 sync_hash = SHA1('req1' + S)
    let mut sync_data = b"req1".to_vec();
    sync_data.extend_from_slice(&s_bytes);
    let sync_hash = sha1_hash(&sync_data);

    // 5. 发送 sync_hash
    stream.write_all(&sync_hash).await?;
    stream.flush().await?;

    // 6. 扫描并消费对方的 sync_hash，精确界定 PadB 边界
    let mut matched = 0;
    let mut bytes_scanned = 0;
    loop {
        let mut b = [0u8; 1];
        stream.read_exact(&mut b).await?;
        if b[0] == sync_hash[matched] {
            matched += 1;
            if matched == 20 { break; }
        } else {
            if b[0] == sync_hash[0] { matched = 1; } else { matched = 0; }
        }
        bytes_scanned += 1;
        if bytes_scanned > 512 + 20 {
            bail!("未能在合理范围内找到 sync_hash，PadB 处理失败");
        }
    }

    // 7. 计算 SKEY 混淆值
    let mut req2 = b"req2".to_vec();
    req2.extend_from_slice(info_hash);
    let hash_req2 = sha1_hash(&req2);
    let mut req3 = b"req3".to_vec();
    req3.extend_from_slice(&s_bytes);
    let hash_req3 = sha1_hash(&req3);
    let mut obfuscated_skey = [0u8; 20];
    for i in 0..20 { obfuscated_skey[i] = hash_req2[i] ^ hash_req3[i]; }

    // 8. 派生 RC4 密钥
    let mut key_a = b"keyA".to_vec();
    key_a.extend_from_slice(&s_bytes);
    key_a.extend_from_slice(info_hash);
    let key_a_hash = sha1_hash(&key_a);
    let mut key_b = b"keyB".to_vec();
    key_b.extend_from_slice(&s_bytes);
    key_b.extend_from_slice(info_hash);
    let key_b_hash = sha1_hash(&key_b);

    let mut encrypt_cipher = Rc4::new(&key_a_hash.into());
    let mut decrypt_cipher = Rc4::new(&key_b_hash.into());
    let mut discard = [0u8; 1024];
    encrypt_cipher.apply_keystream(&mut discard);
    let mut discard2 = [0u8; 1024];
    decrypt_cipher.apply_keystream(&mut discard2);

    // 9. 构造并发送加密协商数据
    let crypto_provide = CRYPTO_RC4.to_be_bytes();
    let mut pad_c = vec![0u8; rng.gen_range(0..512)];
    rng.fill(&mut pad_c[..]);
    let ia = Vec::new();
    let mut payload = Vec::new();
    payload.extend_from_slice(&obfuscated_skey);
    payload.extend_from_slice(&[0u8; 8]);
    payload.extend_from_slice(&crypto_provide);
    payload.extend_from_slice(&(pad_c.len() as u16).to_be_bytes());
    payload.extend_from_slice(&pad_c);
    payload.extend_from_slice(&(ia.len() as u16).to_be_bytes());
    payload.extend_from_slice(&ia);
    encrypt_cipher.apply_keystream(&mut payload);
    stream.write_all(&payload).await?;
    stream.flush().await?;

    // 10. 接收并验证对方的 VC
    let mut vc_buf = [0u8; 8];
    stream.read_exact(&mut vc_buf).await?;
    decrypt_cipher.apply_keystream(&mut vc_buf);
    if vc_buf != [0u8; 8] { bail!("MSE 握手失败: VC 不匹配"); }

    // 11. 读取 crypto_select
    let mut select_buf = [0u8; 4];
    stream.read_exact(&mut select_buf).await?;
    decrypt_cipher.apply_keystream(&mut select_buf);
    let crypto_select = u32::from_be_bytes(select_buf);
    if crypto_select & CRYPTO_RC4 == 0 { bail!("MSE 握手失败: 对方不支持 RC4"); }

    // 12. 读取 PadD
    let mut pad_d_len_buf = [0u8; 2];
    stream.read_exact(&mut pad_d_len_buf).await?;
    decrypt_cipher.apply_keystream(&mut pad_d_len_buf);
    let pad_d_len = u16::from_be_bytes(pad_d_len_buf) as usize;
    if pad_d_len > 0 {
        let mut pad_d = vec![0u8; pad_d_len];
        stream.read_exact(&mut pad_d).await?;
        decrypt_cipher.apply_keystream(&mut pad_d);
    }

    Ok(MseStream::new(stream, key_a_hash, key_b_hash))
}