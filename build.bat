@echo off
chcp 65001 >nul
echo ============================================
echo   便签应用 - 构建脚本
echo ============================================
echo.

echo [1/3] 清理旧编译产物...
cargo clean
echo.

echo [2/3] 编译项目（首次编译较慢，需下载依赖）...
cargo build --release
if %errorlevel% neq 0 (
    echo 编译失败！请检查错误信息。
    pause
    exit /b %errorlevel%
)
echo.

echo [3/3] 构建完成！
echo 可执行文件: target\release\sticky-note.exe
echo.

start target\release\sticky-note.exe
echo 应用已启动！
pause
