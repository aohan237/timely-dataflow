use std::rc::Rc;
use std::cell::RefCell;
use std::default::Default;

use progress::{Timestamp, PathSummary, Scope};
use progress::frontier::Antichain;
use progress::nested::Source::ScopeOutput;
use progress::nested::Target::ScopeInput;
use progress::count_map::CountMap;

use communication::Communicator;
use communication::Observer;
use communication::channels::ObserverHelper;
use communication::channels::{Data, OutputPort};
// use example::stream::Stream;

use example_static::stream::*;
use example_static::builder::*;

pub trait FeedbackExt<G: GraphBuilder> {
    fn feedback<D:Data>(&mut self, limit: G::Timestamp, summary: <G::Timestamp as Timestamp>::Summary) ->
            (FeedbackHelper<ObserverHelper<FeedbackObserver<G::Timestamp, D>>>, Stream<G::Timestamp, D>);
}

impl<G: GraphBuilder> FeedbackExt<G> for G {
    fn feedback<D:Data>(&mut self, limit: G::Timestamp, summary: <G::Timestamp as Timestamp>::Summary) ->
            (FeedbackHelper<ObserverHelper<FeedbackObserver<G::Timestamp, D>>>, Stream<G::Timestamp, D>) {

        let targets = OutputPort::<G::Timestamp, D>::new();
        let produced: Rc<RefCell<CountMap<G::Timestamp>>> = Default::default();
        let consumed: Rc<RefCell<CountMap<G::Timestamp>>> = Default::default();

        let feedback_output = ObserverHelper::new(targets.clone(), produced.clone());
        let feedback_input =  ObserverHelper::new(FeedbackObserver {
            limit: limit, summary: summary, targets: feedback_output, active: false
        }, consumed.clone());

        let index = self.add_scope(FeedbackScope {
            consumed_messages:  consumed.clone(),
            produced_messages:  produced.clone(),
            summary:            summary,
        });

        let helper = FeedbackHelper {
            index:  index,
            target: Some(feedback_input),
        };

        (helper, Stream::new(ScopeOutput(index, 0), targets))
    }
}

enum FeedbackObserverStatus<T: Timestamp> {
    Active(T),
    Inactive,
}

// implementation of the feedback vertex, essentially, as an observer
pub struct FeedbackObserver<T: Timestamp, D:Data> {
    limit:      T,
    summary:    T::Summary,
    targets:    ObserverHelper<OutputPort<T, D>>,
    active:     bool,
    // status:     FeedbackObserverStatus<T>,  // for debugging ideally
}

impl<T: Timestamp, D: Data> Observer for FeedbackObserver<T, D> {
    type Time = T;
    type Data = D;
    #[inline(always)] fn open(&mut self, time: &T) {
        self.active = time.le(&self.limit); // don't send if not less than limit
        // println!("active: {}", self.active);
        if self.active { self.targets.open(&self.summary.results_in(time)); }
    }
    #[inline(always)] fn show(&mut self, data: &D) { if self.active { self.targets.show(data); } }
    #[inline(always)] fn give(&mut self, data:  D) { if self.active { self.targets.give(data); } }
    #[inline(always)] fn shut(&mut self, time: &T) { if self.active { self.targets.shut(&self.summary.results_in(time)); } }
}


// a handy widget for connecting feedback edges
pub struct FeedbackHelper<O: Observer> {
    index:  u64,
    target: Option<O>,
}

impl<O: Observer+'static> FeedbackHelper<O>
where O::Time: Timestamp, O::Data : Data {
    pub fn connect_input<G:GraphBuilder<Timestamp=O::Time>>(&mut self, source: &mut Stream<O::Time, O::Data>, builder: &mut G) -> () {
        source.enable(builder).connect_to(ScopeInput(self.index, 0), self.target.take().unwrap());
    }
}

// the scope that the progress tracker interacts with
pub struct FeedbackScope<T:Timestamp> {
    consumed_messages:  Rc<RefCell<CountMap<T>>>,
    produced_messages:  Rc<RefCell<CountMap<T>>>,
    summary:            T::Summary,
}

impl<T:Timestamp> Scope<T> for FeedbackScope<T> {
    fn name(&self) -> String { format!("Feedback") }
    fn inputs(&self) -> u64 { 1 }
    fn outputs(&self) -> u64 { 1 }

    fn get_internal_summary(&mut self) -> (Vec<Vec<Antichain<T::Summary>>>, Vec<CountMap<T>>) {
        (vec![vec![Antichain::from_elem(self.summary)]], vec![CountMap::new()])
    }

    fn pull_internal_progress(&mut self, _frontier_progress: &mut [CountMap<T>],
                                          messages_consumed: &mut [CountMap<T>],
                                          messages_produced: &mut [CountMap<T>]) -> bool {

        self.consumed_messages.borrow_mut().drain_into(&mut messages_consumed[0]);
        self.produced_messages.borrow_mut().drain_into(&mut messages_produced[0]);
        // println!("feedback pulled: c: {:?}, p: {:?}", messages_consumed[0], messages_produced[0]);
        return false;
    }

    fn notify_me(&self) -> bool { false }
}