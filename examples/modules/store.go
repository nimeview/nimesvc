package store

import "errors"

type User struct {
    Id    string `json:"id"`
    Email string `json:"email"`
}

func GetUser(id string) (User, error) {
    if id == "1" {
        return User{Id: "1", Email: "user@example.com"}, nil
    }
    return User{}, errors.New("not found")
}
