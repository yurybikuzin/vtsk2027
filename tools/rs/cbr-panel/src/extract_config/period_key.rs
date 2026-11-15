pub fn period_key(p: &str) -> (i32, i32) {
    let p = p.trim();
    if let Some((y, q)) = p.split_once('Q') {
        if let (Ok(y), Ok(q)) = (y.parse::<i32>(), q.parse::<i32>()) {
            if (1..=4).contains(&q) {
                return (y, (q - 1) * 3 + 1);
            }
        }
    }
    if let Some((y, m)) = p.split_once('-') {
        if let (Ok(y), Ok(m)) = (y.parse::<i32>(), m.parse::<i32>()) {
            if (1..=12).contains(&m) {
                return (y, m);
            }
        }
    }
    (0, 0)
}
