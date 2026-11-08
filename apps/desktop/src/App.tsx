function App() {
  return (
    <main className="shell">
      <header className="titlebar">
        <div className="brand">
          <span className="brand-mark">IP</span>
          <span>IPMsgSec</span>
        </div>
        <span className="status">
          <span className="status-dot" />
          Desktop shell
        </span>
      </header>

      <section className="welcome">
        <div className="welcome-icon">IP</div>
        <p className="eyebrow">SECURE BY DESIGN · LAN MESSENGER</p>
        <h1>Welcome to IPMsgSec</h1>
        <p className="description">
          The desktop shell is ready. Network session and peer discovery
          integration will be added in the next implementation step.
        </p>
        <div className="hint">
          <strong>Tip</strong>
          <span>
            Closing this window keeps IPMsgSec running in the Windows
            notification area. Use the tray menu to reopen or quit.
          </span>
        </div>
      </section>

      <footer>
        <span>IPMsgSec 0.1.0</span>
        <span>Protocol core · Verus proof boundary unchanged</span>
      </footer>
    </main>
  );
}

export default App;
