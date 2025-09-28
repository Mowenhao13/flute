#!/bin/bash
# simple_flute_test.sh - 最简单的FLUTE测试脚本 (UDP限制兼容版本)

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

FLUTE_DIR="/home/Halllo/Projects/flute"
TEST_DIR="/tmp/simple_flute_test"
UDP_MAX_PAYLOAD=65507

echo -e "${BLUE}🚀 FLUTE 简单测试 (UDP兼容版)${NC}"
echo "========================="
echo -e "${YELLOW}⚠️  注意: encoding_symbol_length 不得超过 $UDP_MAX_PAYLOAD 字节${NC}"

cd "$FLUTE_DIR"

# 检查是否已编译
if [ ! -f "examples/target/release/flute-sender" ]; then
    echo -e "${YELLOW}🔨 编译FLUTE程序...${NC}"
    cd examples
    cargo build --release
    cd ..
fi

# 准备测试目录和文件
echo -e "${YELLOW}📁 准备测试文件...${NC}"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"/{sender,receiver}

# 创建1MB测试文件
dd if=/dev/urandom of="$TEST_DIR/sender/test.bin" bs=1M count=1 2>/dev/null
echo "   创建测试文件: $(ls -lh $TEST_DIR/sender/test.bin | awk '{print $5}')"

# 创建简单配置文件
echo -e "${YELLOW}⚙️ 创建测试配置...${NC}"
cat > "$TEST_DIR/config.yaml" << 'EOF'
sender:
  network:
    destination: "127.0.0.1:3400"
    bind_address: "127.0.0.1"
    bind_port: 0
    send_interval_micros: 1000

  fec:
    type: "no_code"
    encoding_symbol_length: 1400
    maximum_source_block_length: 1024

  flute:
    tsi: 1
    fdt_duration: 2
    inband_cenc: false

  files:
    - path: "test.bin"
      toi: 0
      content_type: "application/octet-stream"

  logging:
    progress_interval: 1000

  advanced:
    object_reemission_periodicity: 2
    keep_partial_files: false
EOF

echo -e "${BLUE}📡 启动接收端...${NC}"
cd examples

# 启动接收端 (后台运行)
RUST_LOG=info ./target/release/flute-receiver \
    127.0.0.1:3400 \
    "$TEST_DIR/receiver" &
RECEIVER_PID=$!

# 等待接收端启动
sleep 2

echo -e "${BLUE}📤 发送文件...${NC}"

# 进入发送目录并发送文件
cd "$TEST_DIR/sender"
RUST_LOG=info "$FLUTE_DIR/examples/target/release/flute-sender" \
    "$TEST_DIR/config.yaml"

echo -e "${YELLOW}⏳ 等待传输完成...${NC}"
sleep 3

# 停止接收端
kill $RECEIVER_PID 2>/dev/null || true
wait $RECEIVER_PID 2>/dev/null || true

# 验证结果
echo -e "${BLUE}🔍 验证传输结果...${NC}"

if [ -f "$TEST_DIR/receiver/test.bin" ]; then
    # MD5校验
    sender_md5=$(md5sum "$TEST_DIR/sender/test.bin" | cut -d' ' -f1)
    receiver_md5=$(md5sum "$TEST_DIR/receiver/test.bin" | cut -d' ' -f1)
    
    echo "发送端MD5: $sender_md5"
    echo "接收端MD5: $receiver_md5"
    
    if [ "$sender_md5" = "$receiver_md5" ]; then
        echo -e "${GREEN}✅ 测试成功! 文件传输完整${NC}"
    else
        echo -e "${RED}❌ MD5不匹配${NC}"
        exit 1
    fi
else
    echo -e "${RED}❌ 接收文件不存在${NC}"
    echo "接收目录内容:"
    ls -la "$TEST_DIR/receiver/" || echo "(目录为空)"
    exit 1
fi

echo -e "${YELLOW}📁 测试文件位置: $TEST_DIR${NC}"
echo -e "${GREEN}🎉 FLUTE测试完成!${NC}"