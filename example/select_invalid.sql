-- vim: filetype=sql
/* select_invalid.sql contains deliberately BROKEN select statements.
   Every statement below should produce a sqleibniz error. This file is
   used to verify that invalid SQL is caught with a clear message instead
   of being silently accepted or causing a crash. */

-- missing result columns
SELECT FROM users;

-- missing table after FROM
SELECT * FROM;

-- missing condition after WHERE
SELECT * FROM users WHERE;

-- unclosed parenthesis
SELECT (1 + 2 FROM users;

-- trailing comma in column list
SELECT name, age, FROM users;

-- missing expression after AND
SELECT * FROM users WHERE age > 18 AND;

-- missing expression in LIMIT
SELECT * FROM users LIMIT;

-- missing expression after ORDER BY
SELECT * FROM users ORDER BY;

-- JOIN without a table
SELECT * FROM users JOIN ON users.id = orders.id;

-- ON without a condition
SELECT * FROM users JOIN orders ON;

-- CASE without END
SELECT CASE WHEN age < 18 THEN 'minor' FROM users;

-- bad CAST: missing AS
SELECT CAST(price INTEGER) FROM products;

-- bad BETWEEN: missing AND
SELECT * FROM t WHERE age BETWEEN 10 20;

-- bad IN: missing closing paren
SELECT * FROM t WHERE id IN (1, 2, 3;

-- CTE with no AS / body
WITH recent SELECT * FROM recent;

-- missing semicolon
SELECT * FROM users
