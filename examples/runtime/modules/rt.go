package modules

import (
	"encoding/json"
	"sync/atomic"
)

type UserV2 struct {
	Id    int64  `json:"id"`
	Name  string `json:"name"`
	Email string `json:"email"`
}

var count int64

func Health() string { return "ok" }

func CreateUser(name string, email string) UserV2 {
	atomic.AddInt64(&count, 1)
	return UserV2{Id: 1, Name: name, Email: email}
}

func GetCount() int64 { return atomic.LoadInt64(&count) }

func GetUser(id int64) UserV2 {
	return UserV2{Id: id, Name: "user", Email: "user@example.com"}
}

type socketSender interface {
	SendRaw(string, any) error
}

func WSJoin(ctx any, _ json.RawMessage) {
	if s, ok := ctx.(socketSender); ok {
		_ = s.SendRaw("MessageOut", map[string]any{"text": "joined"})
	}
}

func WSExit(_ any, _ json.RawMessage) {
	// TODO
}

func WSMessageIn(ctx any, data json.RawMessage) {
	if s, ok := ctx.(socketSender); ok {
		_ = s.SendRaw("MessageOut", map[string]any{"data": json.RawMessage(data)})
	}
}

func WSPing(_ any, _ json.RawMessage) {
	// Pong is automatic
}

func WSMessageOut(_ any, _ json.RawMessage) {}
func WSError(_ any, _ json.RawMessage)     {}
