use crate::dag::service::DagService;

#[derive(Clone)]
pub struct ServiceRegistry {
    pub dag_service: DagService,
}

impl ServiceRegistry {
    pub fn new(dag_service: DagService) -> Self {
        Self { dag_service }
    }
} 