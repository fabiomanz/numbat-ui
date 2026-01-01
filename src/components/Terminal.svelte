<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Terminal } from "xterm";
  import { FitAddon } from "xterm-addon-fit";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import "xterm/css/xterm.css";

  let terminalContainer: HTMLDivElement;
  let term: Terminal;
  let fitAddon: FitAddon;

  onMount(() => {
    let unlisten: () => void;

    term = new Terminal({
      cursorBlink: true,
      fontFamily: '"JetBrains Mono", "Fira Code", Menlo, monospace',
      fontSize: 14,
      allowProposedApi: true,
      macOptionIsMeta: true,
      theme: {
        background: "#000000",
        foreground: "#ffffff",
      },
      // Ensure the terminal does not have extra margins from the renderer
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(terminalContainer);
    fitAddon.fit();

    term.onData((data: string) => {
      invoke("write_to_pty", { data });
    });

    const init = async () => {
      unlisten = await listen<string>(
        "term-data",
        (event: { payload: string }) => {
          term.write(event.payload);
        },
      );

      const backlog = await invoke<string>("init_pty");
      if (backlog) {
        term.write(backlog);
      }

      // Initial fit after spawn
      fitAddon.fit();
      invoke("resize_pty", {
        rows: term.rows,
        cols: term.cols,
      });
      term.focus(); // Focus terminal on start
    };
    init();

    // Resize handler
    const handleResize = () => {
      fitAddon.fit();
      invoke("resize_pty", {
        rows: term.rows,
        cols: term.cols,
      });
    };
    window.addEventListener("resize", handleResize);
    // Timeout to allow layout to settle
    setTimeout(handleResize, 100);

    return () => {
      if (unlisten) unlisten();
      window.removeEventListener("resize", handleResize);
      term.dispose();
    };
  });
</script>

<div bind:this={terminalContainer} class="terminal-container"></div>

<style>
  .terminal-container {
    width: 100vw;
    height: 100vh;
    background-color: #000000; /* Standard Terminal Black */
    /* No user padding, native title bar handles top area */
    padding: 0;
    box-sizing: border-box;
    overflow: hidden;
  }

  /* Make sure transparency works if configured */
  :global(body) {
    margin: 0;
    padding: 0;
    background: #000000;
  }
</style>
