@echo off
for %%f in (*.wicdemo) do (
    echo ===== %%f =====
    python wic_replay_parser.py "%%f"
    echo.
)
pause
