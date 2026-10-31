# 汲湍 v0.5.0 更新说明

本次更新是 P2P 引擎的里程碑，汲湍现在能够真正解析磁力链接、向 Tracker 请求 Peer，并与 Peer 完成 BitTorrent 握手。

## 新增特性
- Magnet 链接解析：支持 xt=urn:btih（40 位十六进制 / 32 位 Base32）提取 20 字节 info_hash，并解析 dn、tr 参数。
- Tracker 请求：实现 BEP-23 Compact Peers 解析（每 Peer 6 字节：4B IPv4 + 2B 大端端口）。
- Peer Wire 握手：实现 BEP-3 标准 68 字节握手帧，验证 info_hash 一致性。
- CLI / GUI 已接入 P2P 测试入口：粘贴磁力链即可看到从解析 → Tracker → 握手的完整链路日志。

## 升级提示
- 运行 `jituan-cli "magnet:?xt=urn:btih:<hash>&dn=<name>"` 即可体验。
- Peer 数据传输与 Piece 校验将在 v0.6.0 实现。