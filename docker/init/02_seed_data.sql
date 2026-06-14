-- poshgre test environment: seed data
-- All data is in English to avoid character encoding issues.

-- ============================================================
-- public schema: users, products, orders
-- ============================================================

INSERT INTO users (username, email, created_at) VALUES
('alice',     'alice@example.com',     '2023-01-05 09:00:00'),
('bob',       'bob@example.com',       '2023-01-10 10:30:00'),
('carol',     'carol@example.com',     '2023-01-15 11:00:00'),
('dave',      'dave@example.com',      '2023-02-01 08:45:00'),
('eve',       'eve@example.com',       '2023-02-10 14:20:00'),
('frank',     'frank@example.com',     '2023-02-15 16:00:00'),
('grace',     'grace@example.com',     '2023-03-01 09:30:00'),
('heidi',     'heidi@example.com',     '2023-03-10 12:00:00'),
('ivan',      'ivan@example.com',      '2023-03-20 10:15:00'),
('judy',      'judy@example.com',      '2023-04-01 13:45:00'),
('kevin',     'kevin@example.com',     '2023-04-05 09:00:00'),
('laura',     'laura@example.com',     '2023-04-10 11:30:00'),
('mike',      'mike@example.com',      '2023-04-15 14:00:00'),
('nancy',     'nancy@example.com',     '2023-05-01 08:00:00'),
('oscar',     'oscar@example.com',     '2023-05-10 10:00:00'),
('peggy',     'peggy@example.com',     '2023-05-15 11:15:00'),
('quinn',     'quinn@example.com',     '2023-06-01 09:45:00'),
('rachel',    'rachel@example.com',    '2023-06-10 13:00:00'),
('steve',     'steve@example.com',     '2023-06-20 15:30:00'),
('tina',      'tina@example.com',      '2023-07-01 09:00:00'),
('ursula',    'ursula@example.com',    '2023-07-05 10:30:00'),
('victor',    'victor@example.com',    '2023-07-10 11:00:00'),
('wendy',     'wendy@example.com',     '2023-07-15 12:00:00'),
('xavier',    'xavier@example.com',    '2023-08-01 09:00:00'),
('yvonne',    'yvonne@example.com',    '2023-08-10 14:00:00'),
('zach',      'zach@example.com',      '2023-08-15 16:00:00'),
('adam',      'adam@example.com',      '2023-09-01 09:00:00'),
('bella',     'bella@example.com',     '2023-09-05 10:00:00'),
('charlie',   'charlie@example.com',   '2023-09-10 11:00:00'),
('diana',     'diana@example.com',     '2023-09-15 12:00:00'),
('edward',    'edward@example.com',    '2023-10-01 09:00:00'),
('fiona',     'fiona@example.com',     '2023-10-05 10:30:00'),
('george',    'george@example.com',    '2023-10-10 11:00:00'),
('hannah',    'hannah@example.com',    '2023-10-15 14:00:00'),
('ian',       'ian@example.com',       '2023-11-01 09:00:00'),
('julia',     'julia@example.com',     '2023-11-05 10:00:00'),
('karl',      'karl@example.com',      '2023-11-10 11:30:00'),
('lisa',      'lisa@example.com',      '2023-11-15 13:00:00'),
('martin',    'martin@example.com',    '2023-12-01 09:00:00'),
('nina',      'nina@example.com',      '2023-12-05 10:00:00'),
('oliver',    'oliver@example.com',    '2023-12-10 11:00:00'),
('paula',     'paula@example.com',     '2023-12-15 12:00:00'),
('richard',   'richard@example.com',   '2024-01-05 09:00:00'),
('sarah',     'sarah@example.com',     '2024-01-10 10:00:00'),
('thomas',    'thomas@example.com',    '2024-01-15 11:00:00'),
('uma',       'uma@example.com',       '2024-02-01 09:00:00'),
('vincent',   'vincent@example.com',   '2024-02-10 10:30:00'),
('wanda',     'wanda@example.com',     '2024-02-15 11:00:00'),
('xander',    'xander@example.com',    '2024-03-01 09:00:00'),
('yara',      'yara@example.com',      '2024-03-10 10:00:00'),
('zeus',      'zeus@example.com',      '2024-03-15 11:00:00'),
('anna',      'anna@example.com',      '2024-04-01 09:00:00'),
('brian',     'brian@example.com',     '2024-04-05 10:00:00'),
('claire',    'claire@example.com',    '2024-04-10 11:00:00'),
('derek',     'derek@example.com',     '2024-04-15 12:00:00'),
('elsa',      'elsa@example.com',      '2024-05-01 09:00:00'),
('fred',      'fred@example.com',      '2024-05-05 10:00:00'),
('gina',      'gina@example.com',      '2024-05-10 11:00:00'),
('hugo',      'hugo@example.com',      '2024-05-15 12:00:00'),
('iris',      'iris@example.com',      '2024-06-01 09:00:00'),
('jake',      'jake@example.com',      '2024-06-05 10:00:00'),
('kate',      'kate@example.com',      '2024-06-10 11:00:00'),
('leo',       'leo@example.com',       '2024-06-15 12:00:00'),
('mia',       'mia@example.com',       '2024-07-01 09:00:00'),
('neil',      'neil@example.com',      '2024-07-05 10:00:00'),
('ora',       'ora@example.com',       '2024-07-10 11:00:00'),
('peter',     'peter@example.com',     '2024-07-15 12:00:00'),
('queen',     'queen@example.com',     '2024-08-01 09:00:00'),
('ross',      'ross@example.com',      '2024-08-05 10:00:00'),
('sue',       'sue@example.com',       '2024-08-10 11:00:00'),
('ted',       'ted@example.com',       '2024-08-15 12:00:00'),
('ula',       'ula@example.com',       '2024-09-01 09:00:00'),
('vera',      'vera@example.com',      '2024-09-05 10:00:00'),
('will',      'will@example.com',      '2024-09-10 11:00:00'),
('xena',      'xena@example.com',      '2024-09-15 12:00:00'),
('york',      'york@example.com',      '2024-10-01 09:00:00'),
('zoe',       'zoe@example.com',       '2024-10-05 10:00:00'),
('alan',      'alan@example.com',      '2024-10-10 11:00:00'),
('betty',     'betty@example.com',     '2024-10-15 12:00:00'),
('carl',      'carl@example.com',      '2024-11-01 09:00:00'),
('dora',      'dora@example.com',      '2024-11-05 10:00:00'),
('earl',      'earl@example.com',      '2024-11-10 11:00:00'),
('flora',     'flora@example.com',     '2024-11-15 12:00:00'),
('glen',      'glen@example.com',      '2024-12-01 09:00:00'),
('holly',     'holly@example.com',     '2024-12-05 10:00:00'),
('igor',      'igor@example.com',      '2024-12-10 11:00:00'),
('jane',      'jane@example.com',      '2024-12-15 12:00:00'),
('kent',      'kent@example.com',      '2025-01-05 09:00:00'),
('luna',      'luna@example.com',      '2025-01-10 10:00:00'),
('max',       'max@example.com',       '2025-01-15 11:00:00'),
('nora',      'nora@example.com',      '2025-02-01 09:00:00'),
('owen',      'owen@example.com',      '2025-02-05 10:00:00'),
('pam',       'pam@example.com',       '2025-02-10 11:00:00'),
('ray',       'ray@example.com',       '2025-02-15 12:00:00'),
('sky',       'sky@example.com',       '2025-03-01 09:00:00'),
('tom',       'tom@example.com',       '2025-03-05 10:00:00'),
('una',       'una@example.com',       '2025-03-10 11:00:00'),
('val',       'val@example.com',       '2025-03-15 12:00:00'),
('wade',      'wade@example.com',      '2025-04-01 09:00:00');

INSERT INTO products (name, price, category, created_at) VALUES
('Wireless Mouse',           29.99, 'Electronics',  '2023-01-01 09:00:00'),
('Mechanical Keyboard',      79.99, 'Electronics',  '2023-01-01 09:00:00'),
('USB-C Hub',                39.99, 'Electronics',  '2023-01-01 09:00:00'),
('Monitor Stand',            49.99, 'Furniture',    '2023-01-01 09:00:00'),
('Desk Lamp',                24.99, 'Furniture',    '2023-01-01 09:00:00'),
('Notebook A5',               9.99, 'Stationery',   '2023-01-01 09:00:00'),
('Ballpoint Pen Set',         4.99, 'Stationery',   '2023-01-01 09:00:00'),
('Sticky Notes Pack',         5.99, 'Stationery',   '2023-01-01 09:00:00'),
('Laptop Backpack',          59.99, 'Bags',         '2023-01-01 09:00:00'),
('Water Bottle 500ml',       14.99, 'Accessories',  '2023-01-01 09:00:00'),
('Noise Cancelling Headphones', 149.99, 'Electronics', '2023-02-01 09:00:00'),
('Webcam 1080p',             69.99, 'Electronics',  '2023-02-01 09:00:00'),
('External SSD 1TB',        109.99, 'Electronics',  '2023-02-01 09:00:00'),
('Ergonomic Chair',         349.99, 'Furniture',    '2023-02-01 09:00:00'),
('Standing Desk Mat',        39.99, 'Furniture',    '2023-02-01 09:00:00'),
('Cable Organizer Set',      12.99, 'Accessories',  '2023-02-01 09:00:00'),
('Phone Stand',              19.99, 'Accessories',  '2023-02-01 09:00:00'),
('Screen Cleaner Kit',        8.99, 'Accessories',  '2023-03-01 09:00:00'),
('Mousepad XL',              22.99, 'Accessories',  '2023-03-01 09:00:00'),
('HDMI Cable 2m',            11.99, 'Electronics',  '2023-03-01 09:00:00'),
('Power Strip 6-outlet',     28.99, 'Electronics',  '2023-03-01 09:00:00'),
('Ethernet Cable 5m',         9.99, 'Electronics',  '2023-03-01 09:00:00'),
('Travel Adapter',           22.99, 'Electronics',  '2023-04-01 09:00:00'),
('Desk Organizer',           17.99, 'Furniture',    '2023-04-01 09:00:00'),
('Whiteboard Small',         34.99, 'Furniture',    '2023-04-01 09:00:00'),
('Marker Set',                6.99, 'Stationery',   '2023-04-01 09:00:00'),
('Binder A4',                 5.99, 'Stationery',   '2023-04-01 09:00:00'),
('Index Cards Pack',          3.99, 'Stationery',   '2023-05-01 09:00:00'),
('Tote Bag',                 12.99, 'Bags',         '2023-05-01 09:00:00'),
('Laptop Sleeve 15in',       24.99, 'Bags',         '2023-05-01 09:00:00'),
('Thermal Mug',              18.99, 'Accessories',  '2023-05-01 09:00:00'),
('Desk Fan USB',             26.99, 'Electronics',  '2023-05-01 09:00:00'),
('Wrist Rest',               14.99, 'Accessories',  '2023-06-01 09:00:00'),
('Document Holder',          21.99, 'Furniture',    '2023-06-01 09:00:00'),
('Label Maker',              39.99, 'Electronics',  '2023-06-01 09:00:00'),
('Stapler',                   8.99, 'Stationery',   '2023-06-01 09:00:00'),
('Scissors',                  4.99, 'Stationery',   '2023-07-01 09:00:00'),
('Tape Dispenser',            6.99, 'Stationery',   '2023-07-01 09:00:00'),
('Filing Cabinet Small',    129.99, 'Furniture',    '2023-07-01 09:00:00'),
('Reading Light Clip',       15.99, 'Accessories',  '2023-07-01 09:00:00'),
('Bluetooth Speaker',        44.99, 'Electronics',  '2023-08-01 09:00:00'),
('Smart Plug',               16.99, 'Electronics',  '2023-08-01 09:00:00'),
('Surge Protector',          22.99, 'Electronics',  '2023-08-01 09:00:00'),
('USB Wall Charger',         19.99, 'Electronics',  '2023-08-01 09:00:00'),
('Tablet Stand Adjustable',  28.99, 'Accessories',  '2023-09-01 09:00:00'),
('Cable Clips Pack',          7.99, 'Accessories',  '2023-09-01 09:00:00'),
('Dry Erase Markers Set',     8.99, 'Stationery',   '2023-09-01 09:00:00'),
('Calendar Desk 2025',       12.99, 'Stationery',   '2023-09-01 09:00:00'),
('Business Card Holder',     11.99, 'Accessories',  '2023-10-01 09:00:00'),
('Bookend Set',              15.99, 'Furniture',    '2023-10-01 09:00:00');

-- Generate ~500 orders using cross-join of users and products
INSERT INTO orders (user_id, product_id, quantity, total, ordered_at)
SELECT
    u.id AS user_id,
    p.id AS product_id,
    1 + (u.id + p.id) % 4 AS quantity,
    p.price * (1 + (u.id + p.id) % 4) AS total,
    '2023-01-01'::timestamp + ((u.id * 5 + p.id * 3) || ' days')::interval AS ordered_at
FROM users u
CROSS JOIN products p
WHERE (u.id + p.id) % 5 < 1   -- keep ~20% of combos to reach ~500 rows
LIMIT 500;

-- ============================================================
-- ecommerce schema
-- ============================================================

INSERT INTO ecommerce.categories (id, name, parent_id) VALUES
(1,  'Electronics',         NULL),
(2,  'Computers',           1),
(3,  'Laptops',             2),
(4,  'Desktops',            2),
(5,  'Peripherals',         2),
(6,  'Smartphones',         1),
(7,  'Audio',               1),
(8,  'Clothing',            NULL),
(9,  'Men',                 8),
(10, 'Women',               8),
(11, 'Kids',                8),
(12, 'Sports',              NULL),
(13, 'Outdoor',             12),
(14, 'Fitness',             12),
(15, 'Home & Garden',       NULL),
(16, 'Kitchen',             15),
(17, 'Furniture',           15),
(18, 'Books',               NULL),
(19, 'Fiction',             18),
(20, 'Non-Fiction',         18);

-- Sync sequence after manual id inserts
SELECT setval('ecommerce.categories_id_seq', (SELECT MAX(id) FROM ecommerce.categories));

INSERT INTO ecommerce.customers (first_name, last_name, email, country, registered_at) VALUES
('James',   'Smith',    'james.smith@mail.com',    'US', '2022-03-10 09:00:00'),
('Mary',    'Johnson',  'mary.johnson@mail.com',   'US', '2022-04-05 10:00:00'),
('Robert',  'Williams', 'robert.w@mail.com',       'CA', '2022-05-01 11:00:00'),
('Patricia','Brown',    'patricia.b@mail.com',     'UK', '2022-06-15 12:00:00'),
('John',    'Jones',    'john.jones@mail.com',     'AU', '2022-07-20 09:00:00'),
('Jennifer','Garcia',   'jennifer.g@mail.com',     'US', '2022-08-10 10:00:00'),
('Michael', 'Martinez', 'michael.m@mail.com',      'US', '2022-09-01 11:00:00'),
('Linda',   'Davis',    'linda.davis@mail.com',    'CA', '2022-10-05 12:00:00'),
('William', 'Miller',   'william.miller@mail.com', 'US', '2022-11-10 09:00:00'),
('Barbara', 'Wilson',   'barbara.w@mail.com',      'UK', '2022-12-15 10:00:00'),
('David',   'Moore',    'david.moore@mail.com',    'US', '2023-01-05 11:00:00'),
('Susan',   'Taylor',   'susan.taylor@mail.com',   'AU', '2023-02-10 12:00:00'),
('Richard', 'Anderson', 'richard.a@mail.com',      'US', '2023-03-01 09:00:00'),
('Jessica', 'Thomas',   'jessica.t@mail.com',      'CA', '2023-04-05 10:00:00'),
('Joseph',  'Jackson',  'joseph.j@mail.com',       'US', '2023-05-10 11:00:00'),
('Sarah',   'White',    'sarah.white@mail.com',    'US', '2023-06-15 12:00:00'),
('Thomas',  'Harris',   'thomas.h@mail.com',       'UK', '2023-07-01 09:00:00'),
('Karen',   'Martin',   'karen.martin@mail.com',   'AU', '2023-08-05 10:00:00'),
('Charles', 'Thompson', 'charles.t@mail.com',      'US', '2023-09-10 11:00:00'),
('Nancy',   'Garcia',   'nancy.garcia@mail.com',   'CA', '2023-10-15 12:00:00');

INSERT INTO ecommerce.items (name, description, price, category_id, stock) VALUES
('ProBook 15 Laptop',       'High performance 15-inch laptop',      1299.99, 3,  50),
('UltraDesk Tower',         'Powerful desktop workstation',          999.99, 4,  30),
('MechaKey RGB',            'RGB mechanical keyboard',                89.99, 5, 200),
('SwiftPhone X12',          'Latest flagship smartphone',            899.99, 6,  75),
('BassBoost Headphones',    'Over-ear noise cancelling headphones',  199.99, 7, 100),
('Classic Polo Shirt',      'Men cotton polo shirt',                  34.99, 9, 300),
('Summer Dress',            'Women floral summer dress',              49.99,10, 250),
('Kids Sneakers',           'Durable kids athletic shoes',            39.99,11, 180),
('Trail Running Shoes',     'Lightweight trail shoes',                89.99,13, 120),
('Yoga Mat Premium',        'Non-slip 6mm yoga mat',                  44.99,14, 200),
('Chef Knife Set',          'Professional 6-piece knife set',        129.99,16,  80),
('Coffee Maker Pro',        'Programmable 12-cup coffee maker',       79.99,16,  60),
('Office Chair Deluxe',     'Ergonomic mesh office chair',           349.99,17,  40),
('Bookshelf 5-tier',        'Solid wood 5-tier bookshelf',           189.99,17,  35),
('Mystery of the Ages',     'Award winning mystery novel',            14.99,19, 500),
('Science of Everything',   'Popular science encyclopedia',           29.99,20, 300),
('Wireless Earbuds Pro',    'True wireless earbuds ANC',             149.99, 7, 150),
('Gaming Mouse',            'High DPI precision gaming mouse',        59.99, 5, 220),
('4K Monitor 27in',         '27-inch 4K UHD IPS monitor',           449.99, 5,  45),
('Smart Watch Series 3',    'Fitness and health tracking watch',     299.99, 6,  90);

INSERT INTO ecommerce.transactions (customer_id, item_id, quantity, amount, purchased_at)
SELECT
    1 + (n % 20) AS customer_id,
    1 + (n % 20) AS item_id,
    1 + (n % 3)  AS quantity,
    i.price * (1 + (n % 3)) AS amount,
    '2023-01-01'::timestamp + (n || ' days')::interval AS purchased_at
FROM generate_series(0, 399) AS n
JOIN ecommerce.items i ON i.id = 1 + (n % 20);

INSERT INTO ecommerce.reviews (customer_id, item_id, rating, comment, created_at)
SELECT
    1 + (n % 20) AS customer_id,
    1 + (n % 20) AS item_id,
    3 + (n % 3)  AS rating,
    CASE n % 5
        WHEN 0 THEN 'Great product, very satisfied.'
        WHEN 1 THEN 'Good quality for the price.'
        WHEN 2 THEN 'Works as expected.'
        WHEN 3 THEN 'Fast shipping and well packaged.'
        ELSE        'Would recommend to others.'
    END AS comment,
    '2023-02-01'::timestamp + ((n * 2) || ' days')::interval AS created_at
FROM generate_series(0, 99) AS n;

-- ============================================================
-- blog schema
-- ============================================================

INSERT INTO blog.authors (name, bio, email) VALUES
('Alice Harper',  'Tech writer and open source contributor.',         'alice.harper@blog.com'),
('Bob Chen',      'Software engineer turned blogger.',                'bob.chen@blog.com'),
('Carol Davis',   'Data scientist and machine learning enthusiast.',  'carol.davis@blog.com'),
('Derek Evans',   'Full-stack developer with 10 years experience.',   'derek.evans@blog.com'),
('Emma Foster',   'UX designer passionate about accessibility.',      'emma.foster@blog.com');

INSERT INTO blog.posts (author_id, title, body, status, published_at) VALUES
(1, 'Getting Started with Rust',
    'Rust is a systems programming language focused on safety and performance. In this post we cover the basics.',
    'published', '2023-03-01 09:00:00'),
(2, 'Building REST APIs with Go',
    'Go provides excellent support for building high-performance REST APIs. Here is a practical guide.',
    'published', '2023-03-10 10:00:00'),
(3, 'Introduction to Machine Learning',
    'Machine learning enables computers to learn from data. This post introduces core concepts.',
    'published', '2023-03-20 11:00:00'),
(4, 'Docker for Developers',
    'Containers have changed how we deploy software. Learn the Docker essentials in this guide.',
    'published', '2023-04-01 09:00:00'),
(5, 'Accessibility in Web Design',
    'Making websites accessible benefits everyone. We explore WCAG guidelines and practical tips.',
    'published', '2023-04-10 10:00:00'),
(1, 'Advanced Rust Patterns',
    'Lifetimes, traits, and generics in Rust can be challenging. This post dives into advanced patterns.',
    'published', '2023-05-01 09:00:00'),
(2, 'Microservices Architecture',
    'Breaking monoliths into services requires careful planning. Here is what we learned.',
    'published', '2023-05-15 10:00:00'),
(3, 'Deep Learning with PyTorch',
    'PyTorch makes deep learning research accessible. Walk through a complete training pipeline.',
    'published', '2023-06-01 11:00:00'),
(4, 'Kubernetes in Production',
    'Running Kubernetes in production involves more than just getting pods running.',
    'published', '2023-06-15 09:00:00'),
(5, 'Design Systems at Scale',
    'A well-maintained design system accelerates product development across teams.',
    'published', '2023-07-01 10:00:00'),
(1, 'Async Rust with Tokio',
    'Tokio is the de facto async runtime for Rust. Learn how to write concurrent programs.',
    'draft', NULL),
(2, 'GraphQL vs REST',
    'Choosing between GraphQL and REST depends on your use case. We compare both approaches.',
    'draft', NULL);

INSERT INTO blog.tags (name) VALUES
('rust'), ('go'), ('python'), ('docker'), ('kubernetes'),
('machine-learning'), ('web-design'), ('accessibility'),
('backend'), ('frontend'), ('devops'), ('database');

INSERT INTO blog.comments (post_id, author_name, body, created_at) VALUES
(1, 'John User',    'Really helpful introduction, thanks!',              '2023-03-02 08:00:00'),
(1, 'Jane Dev',     'Great examples, easy to follow.',                   '2023-03-03 09:00:00'),
(2, 'Mark Go',      'Best Go REST API tutorial I have read.',            '2023-03-11 10:00:00'),
(3, 'Sara ML',      'Clear explanation of the core concepts.',           '2023-03-21 11:00:00'),
(4, 'DevOps Fan',   'Saved me hours of setup time.',                     '2023-04-02 12:00:00'),
(5, 'A11y Ally',    'Accessibility is so important, glad you wrote this.','2023-04-11 09:00:00'),
(6, 'RustAcean',    'The lifetime section is really well explained.',    '2023-05-02 10:00:00'),
(7, 'Arch Reader',  'Good overview of the trade-offs.',                  '2023-05-16 11:00:00'),
(8, 'DL Student',   'The training loop example was very clear.',         '2023-06-02 12:00:00'),
(9, 'K8s Admin',    'Covered a lot of the pain points I experienced.',   '2023-06-16 09:00:00');

INSERT INTO blog.post_tags (post_id, tag_id) VALUES
(1,1),(1,9),(2,2),(2,9),(3,3),(3,6),(4,4),(4,11),(5,7),(5,8),
(6,1),(6,9),(7,9),(7,11),(8,3),(8,6),(9,5),(9,11),(10,7),(10,10);

-- ============================================================
-- analytics schema (base rows only; mass data in 03_generate_events.sql)
-- ============================================================

INSERT INTO analytics.sessions (user_id, started_at, ended_at, page_count)
SELECT
    1 + (n % 1000) AS user_id,
    '2023-01-01'::timestamp + ((n * 7) || ' minutes')::interval AS started_at,
    '2023-01-01'::timestamp + ((n * 7 + 5 + n % 30) || ' minutes')::interval AS ended_at,
    1 + n % 10 AS page_count
FROM generate_series(0, 1999) AS n;

INSERT INTO analytics.page_views (session_id, url, duration_seconds, created_at)
SELECT
    1 + (n % 2000) AS session_id,
    CASE n % 8
        WHEN 0 THEN '/home'
        WHEN 1 THEN '/about'
        WHEN 2 THEN '/products'
        WHEN 3 THEN '/contact'
        WHEN 4 THEN '/blog'
        WHEN 5 THEN '/pricing'
        WHEN 6 THEN '/docs'
        ELSE        '/dashboard'
    END AS url,
    10 + n % 300 AS duration_seconds,
    '2023-01-01'::timestamp + ((n * 3) || ' minutes')::interval AS created_at
FROM generate_series(0, 49999) AS n;

-- ============================================================
-- inventory schema
-- ============================================================

INSERT INTO inventory.warehouses (name, location, capacity) VALUES
('North Hub',   'Chicago, IL',      10000),
('South Hub',   'Dallas, TX',        8000),
('East Hub',    'New York, NY',      9000),
('West Hub',    'Los Angeles, CA',  12000),
('Central Hub', 'Kansas City, MO',   7500);

INSERT INTO inventory.products (sku, name, description, unit_price) VALUES
('SKU-001', 'Steel Bolt M8',         '100-pack M8 hex bolts',              4.99),
('SKU-002', 'Copper Wire 2mm',       '50m roll copper wire',               12.99),
('SKU-003', 'Plastic Container L',   '20L storage container',               8.49),
('SKU-004', 'Safety Gloves L',       'Heat-resistant gloves size L',       14.99),
('SKU-005', 'LED Bulb 10W',          'E27 10W warm white LED',              3.99),
('SKU-006', 'Paint Roller Kit',      'Roller + tray + 2 sleeves',          11.49),
('SKU-007', 'PVC Pipe 1in 3m',       '1-inch diameter PVC pipe 3m',         6.99),
('SKU-008', 'Industrial Tape Roll',  'Heavy duty 50m packing tape',         5.49),
('SKU-009', 'Stainless Shelf 90cm',  'Wall-mount stainless steel shelf',   34.99),
('SKU-010', 'Forklift Battery 24V',  'Replacement 24V forklift battery',  299.99);

INSERT INTO inventory.stock_levels (warehouse_id, product_id, quantity) VALUES
(1,1,500),(1,2,200),(1,3,300),(1,4,150),(1,5,1000),
(2,1,400),(2,2,180),(2,3,250),(2,6,100),(2,7,200),
(3,3,350),(3,4,120),(3,5,800),(3,8,500),(3,9,80),
(4,1,600),(4,5,1200),(4,6,90),(4,9,60),(4,10,20),
(5,2,220),(5,3,280),(5,7,180),(5,8,400),(5,10,15);

INSERT INTO inventory.shipments (warehouse_id, product_id, quantity, shipped_at, destination) VALUES
(1,1, 50,'2024-01-10 08:00:00','Detroit, MI'),
(1,5,200,'2024-01-15 09:00:00','Milwaukee, WI'),
(2,3, 80,'2024-01-20 10:00:00','Houston, TX'),
(3,4, 30,'2024-02-01 08:00:00','Philadelphia, PA'),
(4,9, 10,'2024-02-10 09:00:00','San Francisco, CA'),
(5,2, 40,'2024-02-15 10:00:00','St. Louis, MO'),
(1,2, 60,'2024-03-01 08:00:00','Minneapolis, MN'),
(2,6, 25,'2024-03-10 09:00:00','San Antonio, TX'),
(3,8,100,'2024-03-15 10:00:00','Boston, MA'),
(4,10, 5,'2024-04-01 08:00:00','Phoenix, AZ');

-- ============================================================
-- hr schema
-- ============================================================

INSERT INTO hr.departments (id, name, manager_id) VALUES
(1, 'Engineering',       NULL),
(2, 'Product',           NULL),
(3, 'Design',            NULL),
(4, 'Marketing',         NULL),
(5, 'Human Resources',   NULL),
(6, 'Finance',           NULL),
(7, 'Operations',        NULL),
(8, 'Sales',             NULL);

SELECT setval('hr.departments_id_seq', (SELECT MAX(id) FROM hr.departments));

INSERT INTO hr.employees (first_name, last_name, email, department_id, position, salary, hired_at) VALUES
('James',   'Carter',   'james.carter@corp.com',   1, 'Engineering Manager',   120000.00, '2018-03-01'),
('Emma',    'Lewis',    'emma.lewis@corp.com',      1, 'Senior Engineer',       105000.00, '2019-06-15'),
('Noah',    'Walker',   'noah.walker@corp.com',     1, 'Engineer',               90000.00, '2020-09-01'),
('Olivia',  'Hall',     'olivia.hall@corp.com',     1, 'Engineer',               88000.00, '2021-01-10'),
('Liam',    'Allen',    'liam.allen@corp.com',      1, 'Junior Engineer',        72000.00, '2022-04-01'),
('Sophia',  'Young',    'sophia.young@corp.com',    2, 'Product Manager',       115000.00, '2018-07-01'),
('Mason',   'King',     'mason.king@corp.com',      2, 'Product Owner',         100000.00, '2019-11-01'),
('Ava',     'Wright',   'ava.wright@corp.com',      2, 'Business Analyst',       85000.00, '2020-03-15'),
('Ethan',   'Scott',    'ethan.scott@corp.com',     3, 'Design Lead',           110000.00, '2018-05-01'),
('Isabella','Green',    'isabella.g@corp.com',      3, 'Senior Designer',        95000.00, '2019-08-01'),
('Lucas',   'Baker',    'lucas.baker@corp.com',     3, 'Designer',               78000.00, '2021-02-01'),
('Mia',     'Adams',    'mia.adams@corp.com',       4, 'Marketing Director',    125000.00, '2017-09-01'),
('Aiden',   'Nelson',   'aiden.nelson@corp.com',   4, 'Marketing Manager',     100000.00, '2019-03-01'),
('Harper',  'Hill',     'harper.hill@corp.com',     4, 'Content Writer',         65000.00, '2021-06-01'),
('Logan',   'Ramirez',  'logan.ramirez@corp.com',  5, 'HR Director',           118000.00, '2017-04-01'),
('Elijah',  'Campbell', 'elijah.c@corp.com',        5, 'HR Specialist',          72000.00, '2020-07-01'),
('Abigail', 'Mitchell', 'abigail.m@corp.com',       6, 'CFO',                   180000.00, '2016-01-01'),
('Jackson', 'Perez',    'jackson.p@corp.com',       6, 'Accountant',             75000.00, '2019-05-01'),
('Chloe',   'Roberts',  'chloe.r@corp.com',         7, 'Operations Manager',    105000.00, '2018-10-01'),
('Sebastian','Turner',  'sebastian.t@corp.com',     8, 'Sales Director',        130000.00, '2017-06-01');

-- Set managers (first employee in each department)
UPDATE hr.departments SET manager_id = 1 WHERE id = 1;
UPDATE hr.departments SET manager_id = 6 WHERE id = 2;
UPDATE hr.departments SET manager_id = 9 WHERE id = 3;
UPDATE hr.departments SET manager_id = 12 WHERE id = 4;
UPDATE hr.departments SET manager_id = 15 WHERE id = 5;
UPDATE hr.departments SET manager_id = 17 WHERE id = 6;
UPDATE hr.departments SET manager_id = 19 WHERE id = 7;
UPDATE hr.departments SET manager_id = 20 WHERE id = 8;

INSERT INTO hr.projects (name, department_id, budget, started_at, deadline) VALUES
('Platform Rewrite',        1,  500000.00, '2023-01-01', '2024-06-30'),
('API Gateway v2',          1,  150000.00, '2023-06-01', '2023-12-31'),
('Mobile App Launch',       2,  200000.00, '2023-03-01', '2023-09-30'),
('Q3 Marketing Campaign',   4,   80000.00, '2023-07-01', '2023-09-30'),
('Design System v2',        3,   60000.00, '2023-04-01', '2023-10-31'),
('Annual Budget Planning',  6,   10000.00, '2023-10-01', '2023-12-31'),
('HR Onboarding Portal',    5,   45000.00, '2023-05-01', '2023-11-30'),
('Sales Dashboard',         8,   35000.00, '2023-02-01', '2023-08-31');

INSERT INTO hr.time_entries (employee_id, project_id, hours, work_date, description) VALUES
(1,1,8.0,'2023-01-10','Architecture planning session'),
(2,1,7.5,'2023-01-10','Database schema design'),
(3,1,8.0,'2023-01-10','Initial code scaffolding'),
(4,2,6.0,'2023-06-05','API spec review'),
(5,2,8.0,'2023-06-05','Endpoint implementation'),
(6,3,7.0,'2023-03-05','Product roadmap update'),
(7,3,8.0,'2023-03-05','User story mapping'),
(9,5,8.0,'2023-04-05','Component library setup'),
(10,5,7.5,'2023-04-05','Icon system design'),
(12,4,8.0,'2023-07-05','Campaign strategy meeting'),
(13,4,6.5,'2023-07-05','Content calendar planning'),
(15,7,8.0,'2023-05-05','Requirements gathering'),
(16,7,7.0,'2023-05-05','Vendor evaluation'),
(17,6,8.0,'2023-10-05','Budget data collection'),
(18,6,7.5,'2023-10-05','Financial model review'),
(20,8,8.0,'2023-02-05','Dashboard wireframing'),
(1,1,8.0,'2023-01-11','Sprint planning'),
(2,1,8.0,'2023-01-11','ORM layer implementation'),
(3,2,8.0,'2023-06-06','Rate limiter implementation'),
(4,2,7.0,'2023-06-06','Authentication middleware');
