package modules

import "encoding/json"

func SendMessage(ctx any, data json.RawMessage) {
    c, ok := ctx.(interface {
        JoinRoom(string)
        SendRoom(string, string, any) error
    })
    if !ok {
        return
    }
    c.JoinRoom("chat")
    payload := map[string]any{}
    _ = json.Unmarshal(data, &payload)
    _ = c.SendRoom("chat", "MessageOut", payload)
}

func MessageOut(_ any, _ json.RawMessage) {}
