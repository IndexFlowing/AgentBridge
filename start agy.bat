@echo off
set http_proxy=http://127.0.0.1:7897
set https_proxy=http://127.0.0.1:7897

cd /d "%~dp0"

echo 当前目录：%cd%
echo 代理：%http_proxy%

agy --dangerously-skip-permissions -p "请读取当前项目的 Cargo.toml，只告诉我 package name 和 version，然后退出。"

pause