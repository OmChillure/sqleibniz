-- vim: filetype=sql
/* analyzer.sql exercises the semantic analyzer end-to-end.
   It defines a small schema and then runs queries that exercise:
     - valid usage (should produce 0 errors)
     - unknown tables             (Rule::UnknownTable)
     - unknown columns            (Rule::UnknownColumn)
     - ambiguous columns          (Rule::AmbiguousColumn) -- gap test
     - INSERT value count mismatch (Rule::InsertValueCountMismatch)
     - type mismatch in comparison (Rule::TypeMismatch)

   Run with:
       cargo run -- -i example/analyzer.sql

   The `-i` flag ignores leibniz.lua so every diagnostic is shown. */

-- ============================================================
-- Schema
-- ============================================================
CREATE TABLE users (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL,
    email TEXT UNIQUE
);

CREATE TABLE orders (
    id       INTEGER PRIMARY KEY,
    user_id  INTEGER NOT NULL,
    total    REAL,
    note     TEXT
);

-- ============================================================
-- 1. CORRECT cases (should produce 0 errors)
-- ============================================================

-- simple select
SELECT id, name, email FROM users;

-- qualified column reference
SELECT users.id, users.name FROM users;

-- alias
SELECT u.id, u.email FROM users AS u;

-- join with qualified columns (no ambiguity)
SELECT users.name, orders.total
FROM users
JOIN orders ON users.id = orders.user_id;

-- WHERE with matching types (INTEGER vs INTEGER)
SELECT name FROM users WHERE id = 1;

-- INTEGER vs REAL is allowed (numeric)
SELECT name FROM users WHERE id = 1.0;

-- correct INSERT, explicit columns
INSERT INTO users (id, name, email) VALUES (1, 'alice', 'a@x.com');

-- correct INSERT, all columns implicit
INSERT INTO orders VALUES (1, 1, 19.99, 'first order');

-- UPDATE
UPDATE users SET name = 'bob' WHERE id = 1;

-- DELETE
DELETE FROM orders WHERE id = 1;

-- ============================================================
-- 2. UNKNOWN TABLE (with "did you mean" suggestion)
-- ============================================================

-- typo: userz -> users
SELECT * FROM userz;

-- typo: ordrs -> orders
UPDATE ordrs SET total = 0 WHERE id = 1;

-- typo: ordres -> orders
DELETE FROM ordres WHERE id = 1;

-- ============================================================
-- 3. UNKNOWN COLUMN (with "did you mean" suggestion)
-- ============================================================

-- typo: emial -> email
SELECT emial FROM users;

-- typo: nme -> name (qualified)
SELECT users.nme FROM users;

-- unknown column in WHERE
SELECT id FROM users WHERE phone = '555-1234';

-- unknown column in INSERT column list
INSERT INTO users (id, naem, email) VALUES (2, 'carol', 'c@x.com');

-- unknown column in UPDATE SET
UPDATE users SET nmae = 'dave' WHERE id = 1;

-- ============================================================
-- 4. AMBIGUOUS COLUMN (the gap test)
--    Both `users` and `orders` define `id` -- unqualified `id` is ambiguous.
-- ============================================================

-- unqualified `id` in SELECT projection
SELECT id FROM users JOIN orders ON users.id = orders.user_id;

-- unqualified `id` in WHERE
SELECT users.name FROM users JOIN orders ON users.id = orders.user_id WHERE id = 1;

-- ============================================================
-- 5. INSERT VALUE COUNT MISMATCH
-- ============================================================

-- too few values for explicit column list
INSERT INTO users (id, name, email) VALUES (3, 'eve');

-- too many values for explicit column list
INSERT INTO users (id, name) VALUES (4, 'frank', 'f@x.com', 'extra');

-- too few values for implicit (all columns) form
-- users has 3 columns; only 2 provided
INSERT INTO users VALUES (5, 'grace');

-- too many values for implicit form
INSERT INTO orders VALUES (2, 1, 9.99, 'note', 'extra');

-- ============================================================
-- 6. TYPE MISMATCH in comparison
-- ============================================================

-- INTEGER column compared to string literal
SELECT name FROM users WHERE id = 'hello';

-- REAL column compared to text literal
SELECT id FROM orders WHERE total = 'oops';

-- TEXT column compared to integer literal
SELECT id FROM users WHERE name = 42;

-- mismatch using qualified column
SELECT name FROM users WHERE users.id = 'nope';
