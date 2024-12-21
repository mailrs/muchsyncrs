# Protocol implementation

The muchsyncrs protocol is a line-based protocol, where each line is a JSON object.

Each command is a multi-phased send/receive ceremony that is always initialized by the client:

1. A Command is sent
2. The command is responded to with either a result:
    2.1 Command Result
    2.2 Command Error
3. That result is acknowledged with
    3.1 For Command Result: Command Result Acknowledgement
    3.2 For Command Error: Command Error Acknowledgement
4. if the acknowledgement was an error, the Error Acknowledgement is acknowledged again

The statemachine ("ceremony") looks as follows:

```norun
┌─────────────────┐
│     Command     │
└────────┬────────┘
         │
         │              ┌──────────────────┐
         ├──────────────►   Command Error  ┼───┐
         │              └──────────────────┘   │
         │                                     │
┌────────▼────────┐                            │
│      Reply      │                            │
└────────┬────────┘                            │
         │                                     │
         │               ┌────────────────┐    │
         ┼───────────────►   Reply Error  ┼────┤
         │               └────────────────┘    │
         │                                     │
         │                                     │
┌────────▼────────┐                  ┌─────────▼────┐
│       Fin       │                  │  Error ACK   │
└─────────────────┘                  └──────────────┘
```

Each box in here is "Message" - this is what is send over the wire.
All possible Messages are called an "Operation".
The "order" or "flow" of Messages are encoded in a "Flow".

