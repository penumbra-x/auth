# auth

这是一个适用于`iOS`/`iPad`设备的`HTTP`中间人代理，用于抓取`device_token`

### 前言

最新版的`ChatGPT` APP已上[`SSL pinning`](https://medium.com/trendyol-tech/securing-ios-applications-with-ssl-pinning-38d551945306)验证，使用前提:

- `iOS`/`iPad`设备需要越狱或者已经安装[`巨魔`](https://github.com/opa334/TrollStore)（**越狱后也可以安装**）
- 在[`巨魔`](https://github.com/opa334/TrollStore)商店安装[`TrollFools`](https://github.com/Lessica/TrollFools)，下载[`👉 动态库`](https://github.com/penumbra-x/auth/releases/download/lib/SSLKillSwitch2.dylib)注入到`ChatGPT`

以上只是推荐的方法，当然也有其它方法，目的是绕过[`SSL pinning`](https://medium.com/trendyol-tech/securing-ios-applications-with-ssl-pinning-38d551945306)

### 命令

```bash
$ auth -h
chatgpt preauth devicecheck server

Usage: auth
       auth <COMMAND>

Commands:
  run      Run server
  start    Start server daemon
  restart  Restart server daemon
  stop     Stop server daemon
  log      Show the server daemon log
  ps       Show the server daemon process
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

$ auth run -h
Run server

Usage: auth run [OPTIONS]

Options:
  -d, --debug          Debug mode
  -b, --bind <BIND>    Bind address [default: 0.0.0.0:1080]
  -p, --proxy <PROXY>  Upstream proxy
      --cert <CERT>    MITM server CA certificate file path [default: ca/cert.crt]
      --key <KEY>      MITM server CA private key file path [default: ca/key.pem]
  -h, --help           Print help
```

### 安装

- 编译安装

```bash
# 需要先安装rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

cargo install --git https://github.com/penumbra-x/auth
```

- Docker

```bash
docker run --rm -it -p 1080:1080 ghcr.io/penumbra-x/auth:latest run
```

### 使用

该代理不会像正常代理一样提供正常的网络代理，目的是抓包`device_token`。如果害怕使用多了会被封设备，我建议是使用一些一键换机之类的仿冒设备的软件。

1. 启动服务

- 运行服务

```bash
auth run
# 带代理
auth run --proxy http://192.168.1.1:1080
```

- 守护进程

```bash
auth start
# 带代理
auth start --proxy http://192.168.1.1:1080
```

2. 设置代理

`Wi-Fi`/`Shadowrocket`设置`HTTP`代理

3. 信任证书

浏览器打开`http://192.168.1.100:1080/mitm/cert`，替换你的代理`IP`以及`端口`，打开下载安装以及信任证书。到这里就彻底完成了，由于`Hook`了`ChatGPT`的网络请求，有以下两种抓取更新`device_token`的动作:

- 每次打开和关闭`APP`都会抓取一次，
- 打开`APP`任意点击登录会抓取一次，同理点击取消往复操作也生效。

4. 获取`preauth_cookie`

请求接口`http://192.168.1.100:1080/auth/preauth`，替换你的代理`IP`以及`端口`，示例:

- Request

```bash
curl http://127.0.0.1:1080/auth/preauth
```

- Response
  
```json
{
  "preauth_cookie": "900175BB-61C4-4AA2-B400-4DE3B2E1FD7E:1726892032-9nYJ1mU4JSUAEyhACbVOxYoCATD4uXX8H1HZRJzYQ4E%3D"
}
```

到这里项目的使命已经完成，你可以将`preauth_cookie`用在`ios.chat.openai.com`的接口或者登录。

### 注意

- 自动化操作APP使用不需要太频繁，`cookie`大概会在一段时间内过期（具体不记得什么时间了，24小时？）
- 建议不要把服务放到公网，内网使用Cloudflare [Tunnel](https://www.cloudflare.com/zh-cn/products/tunnel/)开放`/auth/preauth`接口
