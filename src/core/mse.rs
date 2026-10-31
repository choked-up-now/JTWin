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
const VC: [u8; 8] = [0u8; 8];

pub struct MseStream {
    stream: TcpStream,
    encrypt: Rc4<rc4::consts::U20>,
    decrypt: Rc4<rc4::consts::U20>,
    encrypted: bool,
}

impl MseStream {
    pub fn new(stream: TcpStream, encrypt_key: [u8; 20], decrypt_key: [u8; 20]) -> Self {
        let encrypt = Rc4::new(&encrypt_key.into());
        let decrypt = Rc4::new(&decrypt_key.into());
        Self {
            stream,
            encrypt,
            decrypt,
            encrypted: true,
        }
    }

    pub async fn write_encrypted(&mut self, data: &[u8]) -> Result<()> {
        if self.encrypted {
            let mut buf = data.to_vec();
            self.encrypt.apply_keystream(&mut buf);
            self.stream.write_all(&buf).await?;
        } else {
            self.stream.write_all(data).await?;
        }
        self.stream.flush().await?;
        Ok(())
    }

    pub async fn read_decrypted(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = self.stream.read(buf).await?;
        if self.encrypted {
            self.decrypt.apply_keystream(&mut buf[..n]);
        }
        Ok(n)
    }
}

fn sha1_hash(data: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 20];
    out.copy_from_slice(&result);
    out
}

pub async fn mse_handshake_client(mut stream: TcpStream, info_hash: &[u8; 20]) -> Result<MseStream> {
    let p = BigUint::parse_bytes(DH_P_HEX.as_bytes(), 16).context("无效的 DH 素数")?;
    let g = BigUint::from(DH_G);

    // 【核心修复】使用 StdRng 替代 thread_rng，因为 StdRng 是 Send 的，可以安全跨越 await
    let mut rng = StdRng::from_entropy();
    
    let mut xa_bytes = [0u8; 20];
    rng.fill(&mut xa_bytes);
    let xa = BigUint::from_bytes_be(&xa_bytes);
    let ya = g.modpow(&xa, &p);

    let mut ya_bytes = ya.to_bytes_be();
    while ya_bytes.len() < 96 {
        ya_bytes.insert(0, 0);
    }
    let mut pad_a = vec![0u8; rng.gen_range(0..512)];
    rng.fill(&mut pad_a[..]);

    stream.write_all(&ya_bytes).await?;
    stream.write_all(&pad_a).await?;
    stream.flush().await?;

    let mut yb_bytes = [0u8; 96];
    stream.read_exact(&mut yb_bytes).await?;
    let yb = BigUint::from_bytes_be(&yb_bytes);

    let s = yb.modpow(&xa, &p);
    let s_bytes = s.to_bytes_be();

    let mut req2 = b"req2".to_vec();
    req2.extend_from_slice(info_hash);
    let hash_req2 = sha1_hash(&req2);

    let mut req3 = b"req3".to_vec();
    req3.extend_from_slice(&s_bytes);
    let hash_req3 = sha1_hash(&req3);

    let mut obfuscated_skey = [0u8; 20];
    for i in 0..20 {
        obfuscated_skey[i] = hash_req2[i] ^ hash_req3[i];
    }

    let crypto_provide = CRYPTO_RC4.to_be_bytes();
    let mut pad_c = vec![0u8; rng.gen_range(0..512)];
    rng.fill(&mut pad_c[..]);
    let ia = Vec::new();

    let mut payload = Vec::new();
    payload.extend_from_slice(&obfuscated_skey);
    payload.extend_from_slice(&VC);
    payload.extend_from_slice(&crypto_provide);
    payload.extend_from_slice(&(pad_c.len() as u16).to_be_bytes());
    payload.extend_from_slice(&pad_c);
    payload.extend_from_slice(&(ia.len() as u16).to_be_bytes());
    payload.extend_from_slice(&ia);

    stream.write_all(&payload).await?;
    stream.flush().await?;

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

    let mut response = [0u8; 8 + 4 + 2];
    stream.read_exact(&mut response).await?;

    let mut decrypted = response;
    decrypt_cipher.apply_keystream(&mut decrypted);

    if decrypted[0..8] != VC {
        bail!("MSE 握手失败: VC 不匹配");
    }

    let crypto_select = u32::from_be_bytes(decrypted[8..12].try_into()?);
    if crypto_select & CRYPTO_RC4 == 0 {
        bail!("MSE 握手失败: 对方不支持 RC4");
    }

    let len_pad_d = u16::from_be_bytes(decrypted[12..14].try_into()?) as usize;
    let mut pad_d = vec![0u8; len_pad_d];
    stream.read_exact(&mut pad_d).await?;
    decrypt_cipher.apply_keystream(&mut pad_d);

    Ok(MseStream::new(stream, key_a_hash, key_b_hash))
}