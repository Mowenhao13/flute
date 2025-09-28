# FLUTE 文件传输测试完整指南

## 🎯 概述

本指南提供了FLUTE文件传输的完整测试方法，包括本地虚拟网络测试和硬件网络测试两种方案。

## 📋 前提条件

### 系统要求
- Linux系统 (发送端)
- Rust工具链 (cargo, rustc)
- 网络工具 (netstat, tcpdump等)

### 可选硬件测试要求
- Windows接收端 (可选)
- 网线直连或路由器连接

## 🚀 快速测试 (推荐)

### 1. 编译测试程序

```bash
# 进入examples目录
cd /home/Halllo/Projects/flute/examples

# 编译发送端和接收端
cargo build --release

# 检查编译结果
ls target/release/
```

### 2. 准备测试文件

```bash
# 创建测试文件目录
mkdir -p /tmp/flute_test/{sender,receiver}

# 创建小测试文件 (1MB)
dd if=/dev/urandom of=/tmp/flute_test/sender/test_1mb.bin bs=1M count=1

# 创建大测试文件 (10MB)  
dd if=/dev/urandom of=/tmp/flute_test/sender/test_10mb.bin bs=1M count=10

# 验证文件
ls -lh /tmp/flute_test/sender/
```

### 3. 本地环回测试

```bash
# 终端1: 启动接收端 (使用本地环回)
cd /home/Halllo/Projects/flute/examples
./target/release/flute-receiver \
    --bind-address 127.0.0.1:3400 \
    --destination-folder /tmp/flute_test/receiver

# 终端2: 启动发送端
cd /home/Halllo/Projects/flute/examples  
./target/release/flute-sender \
    --config config/config_1mb_no_code_1.yaml \
    --file /tmp/flute_test/sender/test_1mb.bin

# 验证传输结果
ls -la /tmp/flute_test/receiver/
md5sum /tmp/flute_test/sender/test_1mb.bin /tmp/flute_test/receiver/test_1mb.bin
```

## 🔧 配置文件测试

### 使用现有配置文件

FLUTE项目提供了多种预配置的测试方案：

#### NoCode (无编码，最快)
```bash
# 1MB文件测试
./target/release/flute-sender --config config/config_1mb_no_code_1.yaml --file /tmp/test_1mb.bin

# 1024MB文件测试  
./target/release/flute-sender --config config/config_1024mb_no_code_1.yaml --file /tmp/test_1gb.bin
```

#### RaptorQ (高效编码)
```bash
# 1024MB RaptorQ测试
./target/release/flute-sender --config config/config_1024mb_raptorq_1.yaml --file /tmp/test_1gb.bin
```

#### Reed Solomon (高可靠性)
```bash
# Reed Solomon GF28测试
./target/release/flute-sender --config config/config_1024mb_reed_solomon_rs28_1.yaml --file /tmp/test_1gb.bin

# Reed Solomon UnderSpecified测试
./target/release/flute-sender --config config/config_1024mb_reed_solomon_rs28_under_specified_1.yaml --file /tmp/test_1gb.bin
```

### 自定义配置测试

创建自定义配置文件：

```yaml
# custom_test.yaml
sender:
  network:
    destination: "127.0.0.1:3400"
    bind_address: "127.0.0.1"
    bind_port: 0
    send_interval_micros: 1

  fec:
    type: "no_code"
    encoding_symbol_length: 1400
    maximum_source_block_length: 1024

  flute:
    tsi: 1
    fdt_duration: 2
    inband_cenc: false
    
  files:
    - path: "/tmp/flute_test/sender/test_file.bin"
      toi: 0
      content_type: "application/octet-stream"
```

## 🌐 虚拟网络测试 (veth)

使用虚拟网络接口进行隔离测试：

### 1. 设置虚拟网络

```bash
# 创建veth对
sudo ip link add veth0 type veth peer name veth1

# 配置IP地址
sudo ip addr add 192.168.100.1/24 dev veth0  
sudo ip addr add 192.168.100.2/24 dev veth1

# 启动接口
sudo ip link set veth0 up
sudo ip link set veth1 up

# 测试连通性
ping -c 3 192.168.100.2
```

### 2. 使用veth网络测试

```bash
# 接收端 (veth1: 192.168.100.2)
sudo ip netns exec netns1 \
./target/release/flute-receiver \
    --bind-address 192.168.100.2:3400 \
    --destination-folder /tmp/flute_test/receiver

# 发送端 (veth0: 192.168.100.1)  
# 修改配置文件的网络地址为veth网络
./target/release/flute-sender \
    --config config/veth_test.yaml \
    --file /tmp/flute_test/sender/test_file.bin
```

### 3. 清理veth网络

```bash
sudo ip link delete veth0
```

## 🖥️ 硬件网络测试

### 网络拓扑
```
Linux发送端(192.168.1.103) ←→ [网线/路由器] ←→ Windows接收端(192.168.1.102)
```

### 1. 网络配置

#### Linux端配置
```bash
# 配置网卡IP (临时)
sudo ip addr flush dev enp3s0
sudo ip addr add 192.168.1.103/24 dev enp3s0  
sudo ip link set enp3s0 up

# 测试连通性
ping -c 3 192.168.1.102
```

#### Windows端配置
```cmd
# 设置静态IP
# 控制面板 → 网络 → 更改适配器设置 → 以太网属性 → IPv4
# IP地址: 192.168.1.102
# 子网掩码: 255.255.255.0

# 测试连通性  
ping 192.168.1.103
```

### 2. 编译Windows接收端

```bash
# Linux端交叉编译Windows程序
cd /home/Halllo/Projects/flute/examples

# 添加Windows目标平台
rustup target add x86_64-pc-windows-gnu

# 交叉编译
cargo build --release --target x86_64-pc-windows-gnu

# 传输到Windows
scp target/x86_64-pc-windows-gnu/release/flute-receiver.exe \
    administrator@192.168.1.102:"/C:/Users/Administrator/Desktop/flute-receiver.exe"

# 传输配置文件
scp config/config_1024mb_raptorq_1.yaml \
    administrator@192.168.1.102:"/C:/Users/Administrator/Desktop/test_config.yaml"
```

### 3. 执行硬件测试

```bash
# Windows端启动接收 (PowerShell/CMD)
cd C:\Users\Administrator\Desktop  
.\flute-receiver.exe --bind-address 192.168.1.102:3400 --destination-folder C:\temp\flute_received

# Linux端发送文件
cd /home/Halllo/Projects/flute/examples
./target/release/flute-sender \
    --config config/config_1024mb_raptorq_1.yaml \
    --file /tmp/flute_test/sender/test_1gb.bin
```

## 📊 性能测试和监控

### 1. 网络监控

```bash
# 监控网络流量 
sudo tcpdump -i any -n "port 3400"

# 查看网络统计
netstat -su  # UDP统计
ss -u -n     # UDP连接

# 持续监控带宽
iftop -i enp3s0
```

### 2. 传输性能测试

```bash
# 创建不同大小的测试文件
for size in 1 10 100 1024; do
    dd if=/dev/urandom of=/tmp/test_${size}mb.bin bs=1M count=$size
done

# 批量性能测试脚本
#!/bin/bash
for config in config_1024mb_*.yaml; do
    echo "Testing with $config"
    time ./target/release/flute-sender --config "$config" --file /tmp/test_1024mb.bin
    echo "---"
done
```

### 3. 验证传输完整性

```bash
# MD5校验
md5sum /tmp/flute_test/sender/test_file.bin
md5sum /tmp/flute_test/receiver/test_file.bin

# SHA256校验 (更安全)
sha256sum /tmp/flute_test/sender/test_file.bin  
sha256sum /tmp/flute_test/receiver/test_file.bin

# 二进制比较
cmp /tmp/flute_test/sender/test_file.bin /tmp/flute_test/receiver/test_file.bin
```

## 🐛 故障排除

### 常见问题及解决方案

#### 1. 编译错误
```bash
# 更新Rust工具链
rustup update

# 清理并重新编译
cargo clean
cargo build --release
```

#### 2. 网络连接问题
```bash  
# 检查防火墙
sudo ufw status
sudo iptables -L

# 检查端口占用
netstat -tulpn | grep 3400

# 检查路由
ip route show
```

#### 3. 文件传输失败
```bash
# 检查磁盘空间
df -h /tmp

# 检查权限
ls -la /tmp/flute_test/

# 查看程序日志
RUST_LOG=debug ./target/release/flute-sender --config config.yaml --file test.bin
```

#### 4. 性能问题
```bash
# 调整发送间隔 (配置文件中)
send_interval_micros: 100  # 增加间隔降低速率

# 调整符号长度
encoding_symbol_length: 8192  # 增大符号减少包数

# 监控CPU和内存
top
htop
```

## 🔧 高级测试场景

### 1. 多文件传输测试

创建包含多个文件的配置：

```yaml
files:
  - path: "/tmp/file1.bin"
    toi: 1
    content_type: "application/octet-stream"
  - path: "/tmp/file2.bin"
    toi: 2  
    content_type: "application/octet-stream"
```

### 2. 网络损耗模拟

```bash
# 使用netem模拟网络损耗
sudo tc qdisc add dev veth0 root netem loss 5%     # 5%丢包
sudo tc qdisc add dev veth0 root netem delay 10ms  # 10ms延迟

# 清理netem设置
sudo tc qdisc del dev veth0 root netem
```

### 3. 带宽限制测试

```bash
# 限制带宽到10Mbps
sudo tc qdisc add dev veth0 root tbf rate 10mbit burst 32kbit latency 400ms

# 清理带宽限制
sudo tc qdisc del dev veth0 root
```

## 📝 测试报告模板

```markdown
# FLUTE传输测试报告

## 测试环境
- 操作系统: [Linux版本]
- 网络配置: [IP地址/网络拓扑]
- 硬件配置: [CPU/内存/网卡]

## 测试配置  
- FEC方案: [NoCode/RaptorQ/Reed Solomon]
- 文件大小: [MB/GB]
- 符号长度: [字节]
- 冗余率: [百分比]

## 测试结果
- 传输时间: [秒]
- 平均速率: [Mbps]
- 传输成功: [是/否]
- MD5校验: [通过/失败]

## 性能指标
- CPU使用率: [百分比]
- 内存使用: [MB]  
- 网络利用率: [百分比]
- 丢包率: [百分比]

## 问题记录
[遇到的问题和解决方案]
```

## 🎯 自动化测试脚本

### 完整自动化测试

```bash
#!/bin/bash
# flute_auto_test.sh

set -e

# 配置
TEST_DIR="/tmp/flute_auto_test"
SENDER_DIR="$TEST_DIR/sender"
RECEIVER_DIR="$TEST_DIR/receiver"  
FLUTE_DIR="/home/Halllo/Projects/flute/examples"

# 清理和准备
echo "🧹 清理测试环境..."
rm -rf "$TEST_DIR"
mkdir -p "$SENDER_DIR" "$RECEIVER_DIR"

# 创建测试文件
echo "📁 创建测试文件..."
dd if=/dev/urandom of="$SENDER_DIR/test_file.bin" bs=1M count=10 2>/dev/null

# 编译程序
echo "🔨 编译程序..."
cd "$FLUTE_DIR"
cargo build --release --quiet

# 启动接收端 (后台)
echo "📡 启动接收端..."
./target/release/flute-receiver \
    --bind-address 127.0.0.1:3400 \
    --destination-folder "$RECEIVER_DIR" &
RECEIVER_PID=$!

sleep 2

# 发送文件
echo "📤 发送文件..."
./target/release/flute-sender \
    --config config/config_1mb_no_code_1.yaml \
    --file "$SENDER_DIR/test_file.bin"

sleep 3

# 停止接收端
kill $RECEIVER_PID 2>/dev/null || true

# 验证结果
echo "✅ 验证传输结果..."
if [ -f "$RECEIVER_DIR/test_file.bin" ]; then
    echo "文件传输成功!"
    
    # MD5校验
    SENDER_MD5=$(md5sum "$SENDER_DIR/test_file.bin" | cut -d' ' -f1)
    RECEIVER_MD5=$(md5sum "$RECEIVER_DIR/test_file.bin" | cut -d' ' -f1)
    
    if [ "$SENDER_MD5" = "$RECEIVER_MD5" ]; then
        echo "✅ MD5校验通过: $SENDER_MD5"
        echo "🎉 测试成功完成!"
    else
        echo "❌ MD5校验失败!"
        echo "发送端: $SENDER_MD5"  
        echo "接收端: $RECEIVER_MD5"
        exit 1
    fi
else
    echo "❌ 文件传输失败!"
    exit 1
fi

# 清理
echo "🧹 清理测试文件..."
rm -rf "$TEST_DIR"
echo "✨ 测试完成!"
```

保存脚本并运行：

```bash
chmod +x flute_auto_test.sh
./flute_auto_test.sh
```

---

这个完整的测试指南涵盖了从简单的本地测试到复杂的硬件网络测试的所有场景。你可以根据自己的需求选择合适的测试方法！🚀