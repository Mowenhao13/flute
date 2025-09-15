## 虚拟网卡对

```bash
# 创建⼀对虚拟⽹卡（veth-sender ↔ veth-receiver）
sudo ip link add veth-sender type veth peer name veth-receiver

# 给 veth-sender 分配 IP 192.168.100.1
sudo ip addr add 192.168.100.1/24 dev veth-sender
# 给 veth-receiver 分配 IP 192.168.100.2
sudo ip addr add 192.168.100.2/24 dev veth-receiver

sudo ip link set veth-sender up
sudo ip link set veth-receiver up

# 检查 veth-sender 的 IP
ip addr show veth-sender
# 检查 veth-receiver 的 IP
ip addr show veth-receiver
# 清理 veth pair
sudo ip link del veth-sender
```

## RaptorQ批量基准单源块测试
```rust
cargo bench --bench encode_benchmark
cargo bench --bench decode_benchmark
```

## 创建指定大小文件
```bash
dd if=/dev/zero of=test_1024mb.bin bs=1M count=1024
```