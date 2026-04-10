use rocksdb::{ColumnFamilyDescriptor, ColumnFamilyRef, DB, IteratorMode, Options, Snapshot};

pub struct Frontier {
    pub db: DB,
}

impl Frontier {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // DB path delcaration
        let path = "./crawl_data";

        // Column family options
        let cf_ops = Options::default();

        // Column family descriptors
        let seen_links_descriptor = ColumnFamilyDescriptor::new("seen", cf_ops.clone());
        let to_crawl_descriptor = ColumnFamilyDescriptor::new("to_crawl", cf_ops.clone());
        let encountered_domains = ColumnFamilyDescriptor::new("domains", cf_ops.clone());
        let robots_stay_away = ColumnFamilyDescriptor::new("robots", cf_ops.clone());

        // Database options
        let mut db_options = Options::default();

        // Setting the options
        db_options.create_if_missing(true);
        db_options.set_error_if_exists(false);
        db_options.create_missing_column_families(true);

        // Database creation
        let db = DB::open_cf_descriptors(
            &db_options,
            path,
            vec![
                robots_stay_away,
                seen_links_descriptor,
                to_crawl_descriptor,
                encountered_domains,
            ],
        )
        .unwrap();

        Ok(Frontier { db })
    }

    pub fn seen_handle(&self) -> ColumnFamilyRef<'_> {
        self.db
            .cf_handle("seen")
            .expect("Missing seen links cf in the db")
    }

    pub fn to_crawl_handle(&self) -> ColumnFamilyRef<'_> {
        self.db
            .cf_handle("to_crawl")
            .expect("Missing to crawl cf in the db")
    }

    pub fn domain_handle(&self) -> ColumnFamilyRef<'_> {
        self.db
            .cf_handle("domains")
            .expect("Missing domains cf in the db")
    }

    pub fn robots_handle(&self) -> ColumnFamilyRef<'_> {
        self.db
            .cf_handle("robots")
            .expect("Missing robots cf in the db")
    }

    pub fn mark_seen(&self, url: &str) -> Result<(), Box<dyn std::error::Error>> {
        let seen_cf = self.seen_handle();
        let key = url.as_bytes();
        self.db.put_cf(seen_cf, key, &[]).map_err(Into::into)
    }

    pub fn is_seen(&self, url: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let seen_cf = self.seen_handle();
        let key = url.as_bytes();
        if self.db.get_cf(seen_cf, key)?.is_some() {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn add_to_crawl(&self, url: &str) -> Result<(), Box<dyn std::error::Error>> {
        let to_crawl_cf = self.to_crawl_handle();
        let key = url.as_bytes();
        self.db.put_cf(to_crawl_cf, key, &[])?;
        Ok(())
    }

    pub fn delete_from_crawl_cf(&self, key: &str) -> Result<(), Box<dyn std::error::Error>> {
        let to_crawl_cf = self.to_crawl_handle();
        self.db.delete_cf(to_crawl_cf, key)?;
        Ok(())
    }

    pub fn get_from_crawl(&self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        let to_crawl_cf = self.to_crawl_handle();
        match self.db.get_cf(to_crawl_cf, key)? {
            Some(x) => Ok(Some(x)),
            None => Ok(None),
        }
    }

    pub fn add_to_domain(
        &self,
        url: &str,
        time: [u8; 8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let domain_handle = self.domain_handle();
        let key = url.as_bytes();
        self.db.put_cf(domain_handle, key, time)?;
        Ok(())
    }

    pub fn add_to_robots(
        &self,
        domain: &str,
        blob: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let robots_handle = self.robots_handle();
        let key = domain.as_bytes();
        self.db.put_cf(robots_handle, key, blob.as_bytes())?;
        Ok(())
    }

    pub fn get_from_robots(
        &self,
        domain: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let robots_handle = self.robots_handle();
        let key = domain.as_bytes();
        match self.db.get_cf(robots_handle, key)? {
            Some(value) => Ok(Some(String::from_utf8(value)?)),
            None => Ok(None),
        }
    }

    pub fn get_last_visit(&self, host: &str) -> Result<Option<u64>, Box<dyn std::error::Error>> {
        let handle = self.domain_handle();
        let key = host.as_bytes();
        match self.db.get_cf(&handle, key)? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes.as_slice().try_into()?;
                Ok(Some(u64::from_be_bytes(arr)))
            }
            None => Ok(None),
        }
    }

    pub fn get_snapshot(&self) -> Result<Option<Snapshot<'_>>, Box<dyn std::error::Error>> {
        let snapshot = self.db.snapshot();
        let to_crawl_cf = self.to_crawl_handle();
        if snapshot
            .iterator_cf(to_crawl_cf, IteratorMode::Start)
            .next()
            .is_none()
        {
            println!("Processed all links. Hurray!");
            return Ok(None);
        }
        Ok(Some(snapshot))
    }
}

fn main() {
    return;
}
