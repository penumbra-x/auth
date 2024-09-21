# auth

这是一个适用于`iOS`/`iPad`设备的中间人的`HTTP`代理，用于抓取`device_token`

### 前言

最新版的`ChatGPT` APP已上[`SSL pinning`](https://medium.com/trendyol-tech/securing-ios-applications-with-ssl-pinning-38d551945306)验证，使用前提:

- `iOS`/`iPad`设备需要越狱或者已经安装[`巨魔`](https://github.com/opa334/TrollStore)（**越狱后也可以安装**）
- 在巨魔商店安装[`TrollFools`](https://github.com/Lessica/TrollFools)，下载[`👉 动态库`](https://github.com/penumbra-x/auth/releases/download/lib/SSLKillSwitch2.dylib)注入到`ChatGPT`

以上只是推荐的方法，当然也有其它方法，目的是绕过[`SSL pinning`](https://medium.com/trendyol-tech/securing-ios-applications-with-ssl-pinning-38d551945306)

### 使用

该代理不会像正常代理一样提供正常的网络代理，目的是抓包`device_token`。如果害怕使用多了会被封设备，我建议是使用一些一键换机之类的仿冒设备的软件。

1. 设置代理

`Wi-Fi`/`Shadowrocket`设置`HTTP`代理

2. 信任证书

浏览器打开`http://192.168.1.100:1080/mitm/cert`，替换你的代理`IP`以及`端口`，打开下载安装以及信任证书。到这里就彻底完成了，由于`Hook`了`ChatGPT`的网络请求，有以下两种抓取更新`device_token`的动作:

- 每次打开和关闭`APP`都会抓取一次，
- 打开`APP`任意点击登录会抓取一次，同理点击取消往复操作也生效。
