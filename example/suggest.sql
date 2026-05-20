-- vim: filetype=sql
/* suggest.sql exercises the new --suggest quick-fix machinery.
   Each block triggers a diagnostic whose suggestion should print
   under the error when run with:

       cargo run -- --suggest -i example/suggest.sql

   The -i flag ignores leibniz.lua so every diagnostic shows up. */

-- ============================================================
-- Schema (so symbol-aware fixes have something to point at)
-- ============================================================
CREATE TABLE users (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL,
    email TEXT
);

CREATE TABLE orders (
    id       INTEGER PRIMARY KEY,
    user_id  INTEGER NOT NULL,
    total    REAL
);

-- ============================================================
-- 1. = NULL  -> IS NULL
-- ============================================================
SELECT name FROM users WHERE email = NULL;
SELECT name FROM users WHERE email != NULL;

-- ============================================================
-- 2. Reversed BETWEEN bounds
-- ============================================================
SELECT name FROM users WHERE id BETWEEN 100 AND 1;

-- ============================================================
-- 3. LIKE without wildcards
-- ============================================================
SELECT name FROM users WHERE name LIKE 'alice';

-- ============================================================
-- 4. Missing WHERE on UPDATE / DELETE
-- ============================================================
UPDATE users SET name = 'bob';
DELETE FROM orders;

-- ============================================================
-- 5. Trailing comma in column list
-- ============================================================
SELECT id, name, FROM users;

-- ============================================================
-- 6. Closest-match suggestions (already worked, sanity check)
-- ============================================================
SELECT emial FROM users;
SELECT * FROM userz;

-- ============================================================
-- 7. Misspelled keyword
-- ============================================================
SELCT id FROM users;
