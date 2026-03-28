package data

type Audit struct {
    UserId string `json:"user_id"`
    Action string `json:"action"`
}

type Record struct {
    Id string `json:"id"`
    Ok bool `json:"ok"`
}

func AddAudit(userId string, action string) (Audit, error) {
    return Audit{UserId: userId, Action: action}, nil
}

func ListRecords() ([]Record, error) {
    return []Record{{Id: "1", Ok: true}}, nil
}
