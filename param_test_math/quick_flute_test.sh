#!/bin/bash
# quick_flute_test.sh - FLUTE快速测试脚本 (UDP限制兼容版本)

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 配置
FLUTE_DIR="/home/Halllo/Projects/flute"
TEST_DIR="/tmp/quick_flute_test"
RECEIVER_PORT="3400"
UDP_MAX_PAYLOAD=65507

echo -e "${BLUE}🚀 FLUTE 快速测试脚本 (UDP兼容版)${NC}"
echo "================================"
echo -e "${YELLOW}⚠️  UDP数据包限制: 最大载荷 $UDP_MAX_PAYLOAD 字节${NC}"

# 检查项目目录
if [ ! -d "$FLUTE_DIR" ]; then
    echo -e "${RED}❌ 错误: FLUTE项目目录不存在: $FLUTE_DIR${NC}"
    exit 1
fi

cd "$FLUTE_DIR/examples"

# 清理和准备测试目录
echo -e "${YELLOW}🧹 准备测试环境...${NC}"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"/{sender,receiver}

# 创建测试文件
echo -e "${YELLOW}📁 创建测试文件 (5MB)...${NC}"
dd if=/dev/urandom of="$TEST_DIR/sender/test_5mb.bin" bs=1M count=5 2>/dev/null
echo "   测试文件: $(ls -lh $TEST_DIR/sender/test_5mb.bin | awk '{print $5}')"

# 检查并编译程序
echo -e "${YELLOW}🔨 编译FLUTE程序...${NC}"
if [ ! -f "target/release/flute-sender" ] || [ ! -f "target/release/flute-receiver" ]; then
    echo "   正在编译..."
    cargo build --release --quiet
    echo "   编译完成!"
else
    echo "   程序已编译"
fi

# UDP配置检查函数
check_udp_compliance() {
    local config_file="$1"
    echo -e "${BLUE}🔍 检查UDP数据包限制...${NC}"
    
    # 提取encoding_symbol_length值
    if [ -f "$config_file" ]; then
        symbol_length=$(grep "encoding_symbol_length:" "$config_file" | grep -o '[0-9]\+' | head -1)
        
        if [ -n "$symbol_length" ] && [ "$symbol_length" -gt "$UDP_MAX_PAYLOAD" ]; then
            echo -e "${RED}❌ 警告: encoding_symbol_length ($symbol_length) 超过UDP限制 ($UDP_MAX_PAYLOAD)${NC}"
            echo -e "${YELLOW}   这可能导致 'Message too long' 错误${NC}"
            echo -e "${YELLOW}   建议使用 ≤ $UDP_MAX_PAYLOAD 字节的符号长度${NC}"
            return 1
        elif [ -n "$symbol_length" ]; then
            echo -e "${GREEN}✅ UDP兼容: encoding_symbol_length ($symbol_length) ≤ $UDP_MAX_PAYLOAD${NC}"
            if [ "$symbol_length" -le 1472 ]; then
                echo -e "${GREEN}   🌐 标准以太网兼容 (≤1472字节)${NC}"
            fi
            return 0
        fi
    fi
    
    echo -e "${YELLOW}⚠️  无法检测符号长度，请手动验证${NC}"
    return 0
}

# 检查可用配置
echo -e "${YELLOW}📋 可用测试配置:${NC}"
configs=($(ls config/config_*_1.yaml 2>/dev/null | head -5))
if [ ${#configs[@]} -eq 0 ]; then
    echo -e "${RED}❌ 未找到测试配置文件${NC}"
    exit 1
fi

for i in "${!configs[@]}"; do
    config_name=$(basename "${configs[$i]}" | sed 's/config_//; s/_1\.yaml//')
    echo "   $((i+1)). $config_name"
done

# 选择配置或使用默认
echo -e "${BLUE}选择测试配置 (1-${#configs[@]}, 默认1): ${NC}"
read -t 10 choice || choice=1
choice=${choice:-1}

if [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 1 ] && [ "$choice" -le "${#configs[@]}" ]; then
    selected_config="${configs[$((choice-1))]}"
    config_name=$(basename "$selected_config" | sed 's/config_//; s/_1\.yaml//')
    echo -e "${GREEN}✅ 选择配置: $config_name${NC}"
else
    echo -e "${RED}❌ 无效选择，使用默认配置${NC}"
    selected_config="${configs[0]}"
    config_name=$(basename "$selected_config" | sed 's/config_//; s/_1\.yaml//')
fi

# 修改配置文件以使用测试文件
temp_config="$TEST_DIR/test_config.yaml"
cp "$selected_config" "$temp_config"

# 检查UDP兼容性
check_udp_compliance "$temp_config"

# 更新配置文件中的文件路径 (如果配置文件包含files部分)
if grep -q "files:" "$temp_config"; then
    sed -i "s|path: .*|path: \"$TEST_DIR/sender/test_5mb.bin\"|" "$temp_config"
fi

echo -e "${BLUE}🔍 检查网络端口 $RECEIVER_PORT...${NC}"
if netstat -tuln 2>/dev/null | grep -q ":$RECEIVER_PORT "; then
    echo -e "${YELLOW}⚠️  端口 $RECEIVER_PORT 已被占用，尝试终止相关进程${NC}"
    sudo fuser -k $RECEIVER_PORT/udp 2>/dev/null || true
    sleep 1
fi

echo -e "${BLUE}📡 启动接收端...${NC}"
./target/release/flute-receiver \
    --bind-address "127.0.0.1:$RECEIVER_PORT" \
    --destination-folder "$TEST_DIR/receiver" &
RECEIVER_PID=$!

# 等待接收端启动
sleep 2

# 检查接收端是否启动成功
if ! kill -0 $RECEIVER_PID 2>/dev/null; then
    echo -e "${RED}❌ 接收端启动失败${NC}"
    exit 1
fi

echo -e "${BLUE}📤 开始发送文件...${NC}"
echo "   配置: $config_name"
echo "   文件: test_5mb.bin"

# 记录开始时间
START_TIME=$(date +%s)

# 发送文件
if ./target/release/flute-sender \
    --config "$temp_config" \
    --file "$TEST_DIR/sender/test_5mb.bin" 2>/dev/null; then
    
    # 计算传输时间
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    
    echo -e "${GREEN}✅ 发送完成 (${DURATION}秒)${NC}"
else
    echo -e "${RED}❌ 发送失败${NC}"
    kill $RECEIVER_PID 2>/dev/null || true
    exit 1
fi

# 等待接收完成
echo -e "${YELLOW}⏳ 等待接收完成...${NC}"
sleep 3

# 停止接收端
kill $RECEIVER_PID 2>/dev/null || true
wait $RECEIVER_PID 2>/dev/null || true

echo -e "${BLUE}🔍 验证传输结果...${NC}"

# 检查文件是否存在
received_file="$TEST_DIR/receiver/test_5mb.bin"
if [ ! -f "$received_file" ]; then
    echo -e "${RED}❌ 接收文件不存在${NC}"
    echo "   预期位置: $received_file"
    echo "   接收目录内容:"
    ls -la "$TEST_DIR/receiver/" 2>/dev/null || echo "   (目录为空或不存在)"
    exit 1
fi

# 文件大小比较
sender_size=$(stat -c%s "$TEST_DIR/sender/test_5mb.bin")
receiver_size=$(stat -c%s "$received_file")

echo "   发送文件大小: $(numfmt --to=iec $sender_size)"
echo "   接收文件大小: $(numfmt --to=iec $receiver_size)"

if [ "$sender_size" -eq "$receiver_size" ]; then
    echo -e "${GREEN}✅ 文件大小匹配${NC}"
else
    echo -e "${YELLOW}⚠️  文件大小不匹配${NC}"
fi

# MD5校验
echo -e "${YELLOW}🔐 计算MD5校验和...${NC}"
sender_md5=$(md5sum "$TEST_DIR/sender/test_5mb.bin" | cut -d' ' -f1)
receiver_md5=$(md5sum "$received_file" | cut -d' ' -f1)

echo "   发送端 MD5: $sender_md5"
echo "   接收端 MD5: $receiver_md5"

if [ "$sender_md5" = "$receiver_md5" ]; then
    echo -e "${GREEN}✅ MD5校验通过 - 文件完整传输成功!${NC}"
    
    # 计算传输速率
    file_size_mb=$(echo "scale=2; $sender_size / 1024 / 1024" | bc)
    if [ "$DURATION" -gt 0 ]; then
        speed_mbps=$(echo "scale=2; $file_size_mb * 8 / $DURATION" | bc)
        echo -e "${GREEN}📊 传输统计:${NC}"
        echo "   文件大小: ${file_size_mb} MB"
        echo "   传输时间: ${DURATION} 秒"
        echo "   平均速率: ${speed_mbps} Mbps"
    fi
    
else
    echo -e "${RED}❌ MD5校验失败 - 文件传输有误!${NC}"
    exit 1
fi

# 清理选项
echo ""
echo -e "${BLUE}🧹 是否清理测试文件? (y/N): ${NC}"
read -t 10 cleanup || cleanup="N"
if [[ "$cleanup" =~ ^[Yy]$ ]]; then
    rm -rf "$TEST_DIR"
    echo -e "${GREEN}✅ 测试文件已清理${NC}"
else
    echo -e "${YELLOW}📁 测试文件保留在: $TEST_DIR${NC}"
fi

echo ""
echo -e "${GREEN}🎉 FLUTE测试完成!${NC}"

# 显示下一步建议
echo -e "${BLUE}💡 下一步建议:${NC}"
echo "   1. 尝试其他FEC配置 (重新运行此脚本)"
echo "   2. 测试更大文件 (修改脚本中的文件大小)"
echo "   3. 查看详细测试指南: FLUTE_TESTING_GUIDE.md"
echo "   4. 进行网络性能测试"