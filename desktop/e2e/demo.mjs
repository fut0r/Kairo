// A small shop database used to exercise the app end to end.

export const SCHEMA = `// A small shop. Apply it from the Schema workspace, or: kairo create shop

table customers {
  id: int [primary]
  name: string [required]
  email: string [required, unique]
  city: string
  active: bool = true
  joined_at: timestamp
}

table products {
  id: int [primary]
  sku: string [required, unique]
  name: string [required]
  price: float = 0.0 [required]
  in_stock: int = 0
  photo: blob
}

table orders {
  id: int [primary]
  customer_id: int [required]
  status: string = "pending" [required]
  total: float = 0.0
  placed_at: timestamp
}
`;

export const SEED = `CREATE TABLE order_items (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  product_id INTEGER NOT NULL REFERENCES products(id),
  quantity INTEGER NOT NULL DEFAULT 1,
  unit_price REAL NOT NULL
);
CREATE INDEX order_items_order_idx ON order_items (order_id);
CREATE INDEX orders_customer_idx ON orders (customer_id);

WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 64),
  given(k, v) AS (VALUES (0,'Amira'),(1,'Jonas'),(2,'Mei'),(3,'Tariq'),(4,'Sofia'),(5,'Kenji'),
                         (6,'Layla'),(7,'Mateo'),(8,'Nour'),(9,'Elena'),(10,'Omar'),(11,'Ingrid')),
  family(k, v) AS (VALUES (0,'Haddad'),(1,'Lindqvist'),(2,'Tanaka'),(3,'Okafor'),(4,'Rossi'),(5,'Mansour'),
                          (6,'Novak'),(7,'Silva'),(8,'Khalil'),(9,'Becker'),(10,'Yilmaz')),
  town(k, v) AS (VALUES (0,'Cairo'),(1,'Stockholm'),(2,'Osaka'),(3,'Lagos'),(4,'Milan'),(5,'Amman'),(6,'Prague'),(7,'Lisbon'))
INSERT INTO customers (id, name, email, city, active, joined_at)
SELECT i,
       (SELECT v FROM given WHERE k = i % 12) || ' ' || (SELECT v FROM family WHERE k = (i * 7) % 11),
       lower((SELECT v FROM given WHERE k = i % 12)) || '.' || lower((SELECT v FROM family WHERE k = (i * 7) % 11)) || i || '@example.com',
       (SELECT v FROM town WHERE k = (i * 3) % 8),
       i % 9 <> 0,
       datetime('2025-01-06 09:00:00', '+' || (i * 53) || ' hours')
FROM n;
UPDATE customers SET name = 'ليلى حداد', city = NULL WHERE id = 7;

WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 24),
  kind(k, v) AS (VALUES (0,'Notebook'),(1,'Desk Lamp'),(2,'Mechanical Keyboard'),(3,'Monitor Arm'),(4,'USB-C Dock'),(5,'Backpack')),
  tone(k, v) AS (VALUES (0,'Graphite'),(1,'Sand'),(2,'Cobalt'),(3,'Moss'))
INSERT INTO products (id, sku, name, price, in_stock, photo)
SELECT i,
       'SKU-' || printf('%04d', 1000 + i * 37),
       (SELECT v FROM tone WHERE k = (i / 6) % 4) || ' ' || (SELECT v FROM kind WHERE k = i % 6),
       round(9.5 + i * 13.7, 2),
       (i * 17) % 90,
       CASE WHEN i % 5 = 0 THEN x'89504E470D0A1A0A' END
FROM n;

WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 180),
  state(k, v) AS (VALUES (0,'pending'),(1,'paid'),(2,'shipped'),(3,'delivered'),(4,'refunded'))
INSERT INTO orders (id, customer_id, status, total, placed_at)
SELECT i, 1 + (i * 11) % 64, (SELECT v FROM state WHERE k = (i * 7) % 5), 0,
       datetime('2025-03-01 08:00:00', '+' || (i * 19) || ' hours')
FROM n;

WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 420)
INSERT INTO order_items (order_id, product_id, quantity, unit_price)
SELECT 1 + (i * 7) % 180, 1 + (i * 5) % 24, 1 + i % 4,
       (SELECT price FROM products WHERE id = 1 + (i * 5) % 24)
FROM n;

UPDATE orders
   SET total = round((SELECT COALESCE(SUM(quantity * unit_price), 0) FROM order_items WHERE order_id = orders.id), 2)
 WHERE id > 0;

CREATE VIEW customer_spend AS
SELECT c.id, c.name, COUNT(o.id) AS orders, round(COALESCE(SUM(o.total), 0), 2) AS spent
  FROM customers c LEFT JOIN orders o ON o.customer_id = c.id
 GROUP BY c.id;`;

export const REPORT_QUERY = `-- Top customers by what they have spent
SELECT c.name, c.city, COUNT(o.id) AS orders, round(SUM(o.total), 2) AS spent
  FROM customers c
  JOIN orders o ON o.customer_id = c.id
 WHERE o.status <> 'refunded'
 GROUP BY c.id
 ORDER BY spent DESC
 LIMIT 25`;
