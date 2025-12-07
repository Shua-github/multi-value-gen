# multi-value-gen
**感谢[wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen)提供的思路**

## 使用

```bash
multi_value_gen -i <文件路径> -o <文件路径> -f <函数名:返回类型,...;...>
```

### 参数:

- `-i`: 输入`wasm`文件路径
- `-o`: 输出`wasm`文件路径
- `-f`: 函数签名列表,格式为:
  ```
  <函数名:返回类型,...;...>
  ```
  返回值类型有: `i32`, `i64`, `f32`, `f64`
