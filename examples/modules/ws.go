package modules

import "encoding/json"

type ChatSocketContext struct{}

func (c *ChatSocketContext) SendRaw(_ string, _ any) error { return nil }
func (c *ChatSocketContext) SendError(_ string) error     { return nil }

func Join(_ *ChatSocketContext, _ json.RawMessage) {
	// TODO: handle join
}

func Exit(_ *ChatSocketContext, _ json.RawMessage) {
	// TODO: handle exit
}

func MessageIn(_ *ChatSocketContext, _ json.RawMessage) {
	// TODO: handle message in
}

func Typing(_ *ChatSocketContext, _ json.RawMessage) {
	// TODO: handle typing
}

func Ping(_ *ChatSocketContext, _ json.RawMessage) {
	// TODO: handle ping (Pong is sent automatically)
}

func MessageOut(_ *ChatSocketContext, _ json.RawMessage) {
	// TODO: handle message out hook
}

func UserJoined(_ *ChatSocketContext, _ json.RawMessage) {
	// TODO: handle user joined
}

func UserLeft(_ *ChatSocketContext, _ json.RawMessage) {
	// TODO: handle user left
}

func Error(_ *ChatSocketContext, _ json.RawMessage) {
	// TODO: handle error
}

func Notice(_ *ChatSocketContext, _ json.RawMessage) {
	// TODO: handle server notice
}
