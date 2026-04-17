import { useEffect, useRef } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { useSceneStore } from '../store';

export function TerminalPane() {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    // ── Terminal instance ──────────────────────────────────────────────────
    const term = new Terminal({
      theme: {
        background:     '#0a0a0f',
        foreground:     '#e2e8f0',
        cursor:         '#00f5ff',
        cursorAccent:   '#0a0a0f',
        black:          '#1e293b',
        red:            '#ef4444',
        green:          '#22c55e',
        yellow:         '#f59e0b',
        blue:           '#3b82f6',
        magenta:        '#FF1493',
        cyan:           '#00f5ff',
        white:          '#e2e8f0',
        brightBlack:    '#334155',
        brightRed:      '#fca5a5',
        brightGreen:    '#86efac',
        brightYellow:   '#fcd34d',
        brightBlue:     '#93c5fd',
        brightMagenta:  '#f9a8d4',
        brightCyan:     '#67e8f9',
        brightWhite:    '#f8fafc',
      },
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
      fontSize:   14,
      lineHeight: 1.2,
      cursorBlink: true,
      scrollback:  5000,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(containerRef.current);

    // Fit once the element is in the DOM
    setTimeout(() => { try { fitAddon.fit(); } catch (_) {} }, 10);

    // Auto-fit on container or window resize
    const handleResize = () => { try { fitAddon.fit(); } catch (_) {} };
    window.addEventListener('resize', handleResize);
    const observer = new ResizeObserver(handleResize);
    if (containerRef.current) observer.observe(containerRef.current);

    // ── Boot sequence ──────────────────────────────────────────────────────
    term.writeln('\x1b[36m[SYSTEM]\x1b[0m Light Architects Webshell v1.0 initialized.');
    term.writeln('\x1b[38;5;240m[SYSTEM]\x1b[0m Booting terminal interface…');
    term.writeln('\x1b[38;5;240m[SYSTEM]\x1b[0m Connecting to AYIN network graph…');

    setTimeout(() => {
      term.writeln('\x1b[32m[AYIN]\x1b[0m Connection established.');
      term.writeln('Welcome to the SOUL structure explorer.');
      term.writeln('Type \x1b[33m"help"\x1b[0m for a list of commands.');
      term.write('\r\n$ ');
    }, 1500);

    // ── Input handling ─────────────────────────────────────────────────────
    // A ref keeps the current input line in sync even inside async callbacks
    // and Zustand subscriptions where closures would otherwise be stale.
    let currentInput = '';
    const inputRef = { current: '' }; // plain object avoids stale closure

    const syncInput = (val: string) => {
      currentInput = val;
      inputRef.current = val;
    };

    // ── Inject a background log without corrupting the active input line ──
    // Protocol:
    //   \x1b[2K  — erase entire current line
    //   \r        — move to column 0
    //   <message>
    //   \r\n      — new line
    //   $ <restored_input> — reprint prompt + whatever the user had typed
    const injectLog = (msg: string) => {
      term.write(`\x1b[2K\r${msg}\r\n$ ${inputRef.current}`);
    };

    term.onData(e => {
      if (e === '\r') {
        // Enter — execute the command
        term.write('\r\n');
        const cmd = currentInput.trim();

        if (cmd === 'help') {
          term.writeln(
            'Available commands: \x1b[33mhelp\x1b[0m, \x1b[33mclear\x1b[0m, ' +
            '\x1b[33mspawn\x1b[0m, \x1b[33mping\x1b[0m, \x1b[33minfo\x1b[0m, \x1b[33mstatus\x1b[0m'
          );
        } else if (cmd === 'clear') {
          term.clear();
        } else if (cmd === 'spawn') {
          useSceneStore.getState().spawnOrb();
          term.writeln('\x1b[36m[SYSTEM]\x1b[0m Orb entity spawned manually.');
        } else if (cmd === 'ping') {
          term.writeln('\x1b[32mpong\x1b[0m');
        } else if (cmd === 'info') {
          const state = useSceneStore.getState();
          term.writeln(`\x1b[33m[INFO]\x1b[0m Status: ${state.ayinStatus}`);
          term.writeln(`\x1b[33m[INFO]\x1b[0m Active Steps: ${state.steps.length}`);
          term.writeln(`\x1b[33m[INFO]\x1b[0m Active Orbs:  ${state.orbs.length}`);
        } else if (cmd === 'status') {
          const s = useSceneStore.getState();
          const dotChar =
            s.ayinStatus === 'connected'    ? '\x1b[32m●\x1b[0m' :
            s.ayinStatus === 'reconnecting' ? '\x1b[33m●\x1b[0m' : '\x1b[31m●\x1b[0m';
          term.writeln(`${dotChar} AYIN ${s.ayinStatus} · ${s.steps.length} steps · ${s.orbs.length} orbs`);
        } else if (cmd) {
          term.writeln(`\x1b[31mError:\x1b[0m command not found: \x1b[33m${cmd}\x1b[0m`);
        }

        term.write('$ ');
        syncInput('');

      } else if (e === '\x7F') {
        // Backspace
        if (currentInput.length > 0) {
          term.write('\b \b');
          syncInput(currentInput.slice(0, -1));
        }
      } else if (e === '\x03') {
        // Ctrl-C — cancel current line
        term.write('^C\r\n$ ');
        syncInput('');
      } else if (e >= ' ') {
        // Printable characters
        term.write(e);
        syncInput(currentInput + e);
      }
    });

    // ── Zustand subscription — log occasional AYIN trace events ───────────
    let prevOrbCount = 0;

    const unsub = useSceneStore.subscribe(state => {
      if (state.orbs.length > prevOrbCount) {
        // Only log ~20% of orb spawns to avoid spamming
        if (Math.random() > 0.8) {
          const qid = Math.random().toString(36).substring(2, 9);
          injectLog(
            `\x1b[38;5;240m[AYIN]\x1b[0m Incoming trace… (query_id: \x1b[36m${qid}\x1b[0m)`
          );
        }
      }
      prevOrbCount = state.orbs.length;
    });

    // ── Cleanup ────────────────────────────────────────────────────────────
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', handleResize);
      unsub();
      term.dispose();
    };
  }, []);

  return (
    <div
      className="w-full h-full bg-[#0a0a0f] relative overflow-hidden"
      style={{ minWidth: 0 }}
    >
      <div ref={containerRef} className="absolute inset-0 m-2" />
    </div>
  );
}
