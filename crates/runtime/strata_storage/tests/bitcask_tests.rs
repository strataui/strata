use std::fs::OpenOptions;
use std::io::Write;
use strata_storage::bitcask::StrataKV;

#[test]
fn test_torn_write_recovery() {
    let test_wal = std::env::temp_dir().join(format!("strata_test_torn_write_{}.wal", std::process::id()));
    let _ = std::fs::remove_file(&test_wal);

    let original_len = {
        let mut kv = StrataKV::open(&test_wal).expect("Initial open failed");
        kv.put("service_name", b"ingress-router").expect("Put failed");
        kv.put("port", b"8080").expect("Put failed");
        kv.sync().expect("Sync failed");
        assert_eq!(kv.len(), 2);
        kv.total_bytes().expect("total_bytes failed")
    };

    // Simulate a crash/torn write: append 9 corrupted bytes to the WAL tail
    {
        let mut f = OpenOptions::new()
            .write(true)
            .append(true)
            .open(&test_wal)
            .expect("Failed to open for corrupt append");
        f.write_all(b"CORRUPTED").expect("Corrupt write failed");
        f.flush().expect("Flush failed");
    }

    let corrupted_len = std::fs::metadata(&test_wal).unwrap().len();
    assert_eq!(corrupted_len, original_len + 9);

    // Verify integrity before open recognizes the corruption and valid offset
    let (valid_records, valid_offset) = StrataKV::verify_wal_integrity(&test_wal).expect("verify failed");
    assert_eq!(valid_records, 2);
    assert_eq!(valid_offset, original_len);

    // Reopening the database must automatically recover, truncate torn tail, and succeed
    {
        let mut kv = StrataKV::open(&test_wal).expect("Reopen with torn write must succeed via truncation");
        assert_eq!(kv.len(), 2);
        assert_eq!(kv.get("service_name").unwrap(), Some(b"ingress-router".to_vec()));
        assert_eq!(kv.get("port").unwrap(), Some(b"8080".to_vec()));

        // Verify file length was truncated back to original_len
        let truncated_len = kv.total_bytes().unwrap();
        assert_eq!(truncated_len, original_len);

        // Verify we can append new records normally
        kv.put("status", b"recovered").unwrap();
        assert_eq!(kv.len(), 3);
        assert_eq!(kv.get("status").unwrap(), Some(b"recovered".to_vec()));
    }

    let _ = std::fs::remove_file(&test_wal);
}
