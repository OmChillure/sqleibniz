-- vim: filetype=sql
/* select.sql exercises the SELECT statement support: result columns, FROM
   with joins, WHERE, GROUP BY/HAVING, ORDER BY, LIMIT/OFFSET, subqueries,
   CTEs, compound selects and window functions. All statements below are
   valid SQLite and should produce 0 errors. */

-- ---- basic result columns ----
SELECT * FROM users;
SELECT name FROM users;
SELECT name, age FROM users;
SELECT users.* FROM users;
SELECT main.users.id FROM users;
SELECT DISTINCT country FROM users;
SELECT ALL country FROM users;

-- ---- literals and aliases ----
SELECT 1, 'text', NULL, TRUE, FALSE, x'1234';
SELECT 1 + 2 AS sum, 3 * 4 product;
SELECT CURRENT_TIME, CURRENT_DATE, CURRENT_TIMESTAMP;

-- ---- bind parameters ----
SELECT * FROM users WHERE id = ?;
SELECT * FROM users WHERE id = ?1;
SELECT * FROM users WHERE name = :name AND age = @age AND city = $city;

-- ---- operator precedence ----
SELECT * FROM users WHERE a OR b AND c;
SELECT * FROM t WHERE x = 1 AND y = 2 OR z = 3;
SELECT * FROM t WHERE NOT active AND age > 18;
SELECT * FROM t WHERE a + b * c - d / e;
SELECT * FROM t WHERE name LIKE 'a%' AND age BETWEEN 10 AND 20;
SELECT * FROM t WHERE id IN (1, 2, 3);
SELECT * FROM t WHERE col IS NULL OR other IS NOT NULL;

-- ---- WHERE / GROUP BY / HAVING ----
SELECT country, COUNT(*) AS c FROM users WHERE age > 18 GROUP BY country HAVING c > 5;

-- ---- ORDER BY / LIMIT / OFFSET ----
SELECT name FROM users ORDER BY name ASC, age DESC;
SELECT name FROM users LIMIT 10;
SELECT name FROM users LIMIT 10 OFFSET 5;
SELECT name FROM users LIMIT 5, 10;

-- ---- joins ----
SELECT u.name, o.total
FROM users u
JOIN orders o ON o.user_id = u.id
LEFT JOIN payments p ON p.order_id = o.id
WHERE o.total > 100;

-- ---- CASE and CAST ----
SELECT CASE WHEN age < 18 THEN 'minor' ELSE 'adult' END FROM users;
SELECT CASE age WHEN 1 THEN 'one' WHEN 2 THEN 'two' ELSE 'many' END FROM users;
SELECT CAST(price AS INTEGER) FROM products;

-- ---- subqueries ----
SELECT * FROM users WHERE id IN (SELECT user_id FROM orders);
SELECT * FROM users u WHERE EXISTS (SELECT 1 FROM orders o WHERE o.user_id = u.id);
SELECT (SELECT COUNT(*) FROM orders) AS order_count;

-- ---- CTEs ----
WITH recent AS (SELECT * FROM logs WHERE ts > 0)
SELECT * FROM recent;

WITH RECURSIVE counter(n) AS (
    SELECT 1
    UNION ALL
    SELECT n + 1 FROM counter WHERE n < 10
)
SELECT n FROM counter;

-- ---- compound selects ----
SELECT id FROM users UNION SELECT id FROM archive;
SELECT id FROM users UNION ALL SELECT id FROM archive;
SELECT id FROM a INTERSECT SELECT id FROM b;
SELECT id FROM a EXCEPT SELECT id FROM b;

-- ---- window functions ----
SELECT name, ROW_NUMBER() OVER (ORDER BY age) FROM users;
SELECT name, RANK() OVER (PARTITION BY country ORDER BY age DESC) FROM users;

-- ---- VALUES ----
SELECT * FROM (VALUES (1, 'a'), (2, 'b'));

-- ---- kitchen sink ----
WITH active_users AS (
    SELECT id, name, country FROM users WHERE active = TRUE
)
SELECT au.country, COUNT(*) AS total
FROM active_users au
JOIN orders o ON o.user_id = au.id
WHERE o.total > 50
GROUP BY au.country
HAVING total > 3
ORDER BY total DESC
LIMIT 20;

-- ---- EXPLAIN with SELECT ----
EXPLAIN QUERY PLAN SELECT * FROM users WHERE id = 1;
