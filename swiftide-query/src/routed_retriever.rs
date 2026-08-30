use std::sync::Arc;

use async_trait::async_trait;
use swiftide_core::{
    ConcreteRetrieveFn, Retrieve, RetrieveConcrete as _, SearchStrategy,
    querying::search_strategies::Multiple,
};

#[derive(Default)]
pub struct RoutedRetriever {
    routes: Arc<Vec<Route>>,
}

pub struct Route {
    name: String,
    description: String,
    retrieve_fn: ConcreteRetrieveFn,
}

impl RoutedRetriever {
    /// Adds a new route to the retriever
    ///
    /// The name and description can be used by the LLM to select the appropriate route.
    pub fn add_route<S: SearchStrategy + 'static>(
        &mut self,
        name: String,
        description: String,
        search_strategy: &S,
        retriever: impl Retrieve<S> + 'static,
    ) {
        let retriever = Arc::new(retriever);
        let retrieve_fn = retriever.concrete_retrieve_fn(search_strategy);

        let route = Route {
            name,
            description,
            retrieve_fn: Box::new(retrieve_fn),
        };
        self.routes.push(route);
    }
}

// Or just make it work with any? It doesn't really matter
//
#[async_trait]
impl Retrieve<Multiple> for RoutedRetriever {
    async fn retrieve(
        &self,
        search_strategy: &Multiple,
        query: swiftide_core::Query<swiftide_core::states::Pending>,
    ) -> anyhow::Result<swiftide_core::Query<swiftide_core::states::Retrieved>> {
        // Find the route that matches the search strategy
        let route = self
            .routes
            .iter()
            .find(|r| r.name == search_strategy.name)
            .ok_or_else(|| {
                anyhow::anyhow!("No route found for strategy: {}", search_strategy.name)
            })?;

        // Call the retrieve function for the matched route
        (route.retrieve_fn)(&query).await
    }
}
