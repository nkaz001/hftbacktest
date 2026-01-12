# Rust 基础知识

本章节为 Python 开发者介绍 Rust 的核心概念，帮助你快速上手策略开发。

## 安装 Rust

```bash
# 安装 rustup (Rust 版本管理器)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 重新加载环境变量
source $HOME/.cargo/env

# 验证安装
rustc --version
cargo --version
```

## Rust vs Python 语法对比

### 变量声明

```python
# Python
x = 10
x = "hello"  # 可以改变类型
```

```rust
// Rust
let x = 10;          // 不可变变量
let mut y = 10;      // 可变变量
y = 20;              // OK
// y = "hello";      // 错误: 类型不匹配
```

### 类型系统

```python
# Python - 动态类型
def add(a, b):
    return a + b
```

```rust
// Rust - 静态类型
fn add(a: f64, b: f64) -> f64 {
    a + b
}

// 常用类型
// f64: 双精度浮点 (Python 的 float)
// i64: 64位整数 (Python 的 int)
// usize: 无符号整数 (用于索引)
// bool: 布尔值
// String: 字符串
// Vec<T>: 动态数组 (Python 的 list)
// HashMap<K,V>: 哈希表 (Python 的 dict)
```

### 结构体 (类)

```python
# Python
class Order:
    def __init__(self, price, qty):
        self.price = price
        self.qty = qty

    def value(self):
        return self.price * self.qty
```

```rust
// Rust
struct Order {
    price: f64,
    qty: f64,
}

impl Order {
    fn new(price: f64, qty: f64) -> Self {
        Self { price, qty }
    }

    fn value(&self) -> f64 {
        self.price * self.qty
    }
}

// 使用
let order = Order::new(100.0, 1.5);
println!("Value: {}", order.value());
```

### 循环

```python
# Python
for i in range(10):
    print(i)

for item in items:
    print(item)
```

```rust
// Rust
for i in 0..10 {
    println!("{}", i);
}

for item in &items {
    println!("{}", item);
}

// 带索引
for (i, item) in items.iter().enumerate() {
    println!("{}: {}", i, item);
}
```

### 条件语句

```python
# Python
if x > 0:
    result = "positive"
elif x < 0:
    result = "negative"
else:
    result = "zero"
```

```rust
// Rust
let result = if x > 0 {
    "positive"
} else if x < 0 {
    "negative"
} else {
    "zero"
};
```

### Option 和 None

```python
# Python
def find_order(order_id):
    if order_id in orders:
        return orders[order_id]
    return None

# 使用
order = find_order(123)
if order is not None:
    print(order.price)
```

```rust
// Rust - 使用 Option<T> 表示可能为空
fn find_order(order_id: u64) -> Option<&Order> {
    orders.get(&order_id)
}

// 使用
if let Some(order) = find_order(123) {
    println!("{}", order.price);
}

// 或者使用 match
match find_order(123) {
    Some(order) => println!("{}", order.price),
    None => println!("Order not found"),
}

// 或者提供默认值
let price = find_order(123).map(|o| o.price).unwrap_or(0.0);
```

### 错误处理

```python
# Python
try:
    result = risky_operation()
except Exception as e:
    print(f"Error: {e}")
```

```rust
// Rust - 使用 Result<T, E>
fn risky_operation() -> Result<f64, String> {
    if condition {
        Ok(42.0)
    } else {
        Err("Something went wrong".to_string())
    }
}

// 使用
match risky_operation() {
    Ok(value) => println!("Success: {}", value),
    Err(e) => println!("Error: {}", e),
}

// 使用 ? 操作符传播错误
fn caller() -> Result<(), String> {
    let value = risky_operation()?;  // 如果错误则提前返回
    println!("Got: {}", value);
    Ok(())
}
```

### 集合操作

```python
# Python
prices = [100.0, 101.0, 102.0]
avg = sum(prices) / len(prices)
high_prices = [p for p in prices if p > 100.5]
```

```rust
// Rust
let prices = vec![100.0, 101.0, 102.0];
let avg: f64 = prices.iter().sum::<f64>() / prices.len() as f64;
let high_prices: Vec<f64> = prices.iter()
    .filter(|&&p| p > 100.5)
    .copied()
    .collect();
```

## 所有权系统 (Rust 核心概念)

Rust 最重要的特性是所有权系统，它保证内存安全而无需垃圾回收。

### 基本规则

1. 每个值有且只有一个所有者
2. 当所有者离开作用域，值被释放
3. 可以借用 (`&`) 而不获取所有权

```rust
// 移动 (Move)
let s1 = String::from("hello");
let s2 = s1;  // s1 的所有权移动到 s2
// println!("{}", s1);  // 错误: s1 已经无效

// 借用 (Borrow)
let s1 = String::from("hello");
let len = calculate_length(&s1);  // 借用 s1
println!("{} has length {}", s1, len);  // s1 仍然有效

fn calculate_length(s: &String) -> usize {
    s.len()
}

// 可变借用
fn append_exclaim(s: &mut String) {
    s.push('!');
}

let mut s = String::from("hello");
append_exclaim(&mut s);
println!("{}", s);  // "hello!"
```

### 策略开发中的应用

```rust
// 在策略中，通常使用引用来访问市场数据
fn update(&mut self, bot: &mut impl Bot<MD>, asset_no: usize) {
    // bot 是可变借用，我们可以读取和修改它
    let depth = bot.depth(asset_no);  // 获取深度数据的引用
    let best_bid = depth.best_bid();

    // 提交订单
    bot.submit_buy_order(asset_no, order_id, price, qty, ...);
}
```

## 项目结构

```
strategies/obi_mm_rust/
├── Cargo.toml          # 项目配置 (类似 package.json / pyproject.toml)
├── src/
│   ├── lib.rs          # 库入口
│   ├── config.rs       # 配置结构
│   ├── strategy.rs     # 策略逻辑
│   ├── backtest.rs     # 回测入口 (bin)
│   └── testnet.rs      # 实盘入口 (bin)
└── config.example.toml # 配置模板
```

### Cargo.toml 示例

```toml
[package]
name = "obi_mm"
version = "0.1.0"
edition = "2021"

[lib]
name = "obi_mm"
path = "src/lib.rs"

[[bin]]
name = "backtest"
path = "src/backtest.rs"

[[bin]]
name = "obi_mm_testnet"
path = "src/testnet.rs"

[dependencies]
hftbacktest = { path = "../../hftbacktest" }
factor-engine = { path = "../../factor-engine" }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
tracing = "0.1"
anyhow = "1"
```

## 常用命令

```bash
# 编译检查 (不生成二进制)
cargo check

# 编译 (debug 模式)
cargo build

# 编译 (release 模式，优化)
cargo build --release

# 运行
cargo run --release --bin backtest -- --help

# 运行测试
cargo test

# 格式化代码
cargo fmt

# 代码检查 (lint)
cargo clippy
```

## 推荐学习资源

- [Rust Book (官方教程)](https://doc.rust-lang.org/book/) - 免费在线教程
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/) - 示例驱动学习
- [Rustlings](https://github.com/rust-lang/rustlings) - 交互式练习

## 下一步

现在你已经了解了 Rust 基础，接下来阅读 [Python 到 Rust 策略迁移](02-python-to-rust.md) 学习如何将 Python 策略转换为 Rust。
