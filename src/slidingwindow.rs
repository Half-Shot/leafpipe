pub struct SlidingWindow {
    recorded_intensites: Vec<f32>,
    min: f32,
    max: f32,
    updates: usize,
    limit: usize,
}

impl SlidingWindow {
    pub fn new(limit: usize) -> Self {
        SlidingWindow {
            updates: 0,
            recorded_intensites: Vec::new(),
            min: 100.0f32,
            max: 0.0f32,
            limit,
        }
    }

    pub fn submit_new(&mut self, value: f32) -> (f32, f32) {
        if value < 0.1f32 {
            return (
                self.min,
                self.max,
            );
        }
        self.updates += 1;
        self.recorded_intensites.push(value);
        if self.recorded_intensites.len() > self.limit {
            self.recorded_intensites.pop();
        }
        if value > self.max {
            self.max = value;
        }
        if value < self.min {
            self.min = value;
        }
        if self.updates > self.limit {
            self.updates = 0;

            self.max = *self.recorded_intensites
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b)).unwrap().1;

            self.min = *self.recorded_intensites
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.total_cmp(b)).unwrap().1;

        }
        (
           self.min,
           self.max,
        )
    }
    
}