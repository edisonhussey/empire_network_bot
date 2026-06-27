from mitmproxy import http
from datetime import datetime
import os
import sys
import threading
import time

CAPTURE_FOLDER = "captures"
os.makedirs(CAPTURE_FOLDER, exist_ok=True)

current_file = None
last_dump = datetime.now()

def websocket_message(flow: http.HTTPFlow):
    global current_file, last_dump
    
    msg = flow.websocket.messages[-1]
    direction = "CLIENT →" if msg.from_client else "SERVER ←"
    ts = datetime.now()
    
    content = msg.content
    decoded = None
    if msg.is_text:
        decoded = content.decode('utf-8', errors='replace')
    else:
        try:
            decoded = content.decode('utf-8', errors='replace')
        except:
            decoded = content.hex()
    
    if current_file is None or (ts - last_dump).total_seconds() >= 10:
        if current_file:
            current_file.close()
        filename = f"{CAPTURE_FOLDER}/gge_{ts.strftime('%Y%m%d_%H%M%S')}.log"
        current_file = open(filename, "a", encoding="utf-8")
        last_dump = ts
        print(f"📁 New capture: {filename}")
    
    entry = f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction}\n{decoded}\n---\n"
    current_file.write(entry)
    current_file.flush()
    
    if "%xt%" in str(decoded) or "gaa" in str(decoded):
        print(f"\n🔥 GAME PACKET [{direction}]\n{decoded[:800]}{'...' if len(str(decoded)) > 800 else ''}")

def websocket_start(flow: http.HTTPFlow):
    print(f"✅ Connected: {flow.request.url}")

def done():
    if current_file:
        current_file.close()

# Live Sender
def terminal_sender():
    print("\n" + "="*60)
    print("Type full %xt% packet and press ENTER to send")
    print("Example: %xt%EmpireEx_19%msd%1%{\"X\":644,\"Y\":639,\"MST\":\"MS2\"}%")
    print("="*60 + "\n")
    
    while True:
        try:
            line = input("SEND> ").strip()
            if line:
                print(f"Sending: {line[:150]}...")
                # TODO: Send through the websocket connection (needs more setup)
                print("→ Sent (placeholder)")
        except:
            break

threading.Thread(target=terminal_sender, daemon=True).start()

addons = []
