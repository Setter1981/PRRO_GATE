# Wiring 1C:Enterprise to the Virtual Maria 304 Driver

1C talks to fiscal registers through the Resonance OLE Manager DLL
(`Resonance.OLEManager.dll`).  That DLL accepts either a COM port
name (`COM4`) or a TCP address (`tcp://127.0.0.1:9100`) as the first
argument to `Init`/`InitEx`.  The driver implements the same wire
protocol as a physical Maria 304, so the OLE Manager cannot tell the
difference.

## Single-host deployment

Driver and 1C on the same Windows machine.

1. Open your 1C configuration.  Find the "Підключення фіскального
   реєстратора" settings (module name varies — usually "Обладнання"
   → "Фіскальні реєстратори").
2. Change the connection string from `COM4` (or whatever) to:

   ```
   tcp://127.0.0.1:9100
   ```

   where `9100` matches `listeners[0].bind` in the driver's config.

3. Keep the cashier name + password fields as before.  Driver
   accepts `"1111111111"` by default; change via
   `listeners[].cashier_password`.
4. Save + restart the 1C session.  The first fiscal operation hits
   the driver.

## Centralised deployment

Multiple cashier stations, driver on a shop server.

1. Pick a reachable IP for the server — e.g. `192.168.1.10`.
2. On the server, bind each listener to `0.0.0.0:<port>`.
3. In 1C on each cashier station, use:

   ```
   tcp://192.168.1.10:9100
   ```

   with the port matching the FN you want that cashier to drive.
4. Firewall the server so only cashier stations can reach the
   listener ports.  Admin port (9202) must stay LAN-only.

## Diagnostic first-run

Before going live:

* Start the driver in **dry-run** mode (`deployment.mode: dry-run`).
  Drive a few SELL/RETURN through 1C.  Check the journal — no
  canonical submits happen but every frame is logged with
  `mode="dry-run"`.
* Flip to **shadow** mode — envelope round-trips to Python, but
  Python does not submit to DPS.  Use this to validate the
  canonical schema without any fiscal risk.
* Only then switch to **live**.
