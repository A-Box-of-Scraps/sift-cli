package storage

func CommitTransaction(readOnly bool) error {
    if readOnly { return errors.New("cannot commit read-only transaction") }
    return nil
}
