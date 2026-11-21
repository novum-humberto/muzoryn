//! Gestor de descargas: estado de los trabajos (sin hilos ni red).
//!
//! 6.9.7-Pro: modo de descarga elegible (`Parallel`: hasta 8 a la vez; `Queue`: de una en una).
//! 6.9.3-Pro: límite real de `MAX_PARALLEL` descargas activas; el resto espera en cola y
//! arranca solo cuando se libera un hueco. `App` lanza los hilos con `next_to_start()`.

/// Máximo de descargas simultáneas.
pub const MAX_PARALLEL: usize = 8;

/// Cómo se reparten los huecos de descarga.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DownloadMode {
    /// Hasta `MAX_PARALLEL` descargas a la vez.
    #[default]
    Parallel,
    /// Una sola descarga activa; el resto espera su turno en orden.
    Queue,
}

impl DownloadMode {
    /// Máximo de descargas activas en este modo.
    pub fn limit(self) -> usize {
        match self {
            DownloadMode::Parallel => MAX_PARALLEL,
            DownloadMode::Queue => 1,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DownloadMode::Parallel => "en paralelo",
            DownloadMode::Queue => "en cola (una a una)",
        }
    }
}

/// Progreso global: media de los trabajos finalizados y de las fracciones actuales. Una entrada
/// en cola o sin tamaño conocido cuenta como 0. `None` si no hay trabajos en la sesión.
pub fn overall<I: IntoIterator<Item = Option<f64>>>(fracs: I) -> Option<f64> {
    let (sum, n) = fracs.into_iter().fold((0.0, 0usize), |(s, n), f| {
        (s + f.unwrap_or(0.0).clamp(0.0, 1.0), n + 1)
    });
    (n > 0).then(|| sum / n as f64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Active,
}

/// Un episodio en cola o descargándose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadJob {
    pub id: u64,
    pub title: String,
    pub podcast: String,
    pub url: String,
    pub state: JobState,
    pub received: u64,
    pub size: Option<u64>,
}

impl DownloadJob {
    /// Fracción descargada (0.0 a 1.0) si se conoce el tamaño total.
    pub fn fraction(&self) -> Option<f64> {
        self.size
            .filter(|s| *s > 0)
            .map(|s| (self.received as f64 / s as f64).min(1.0))
    }

    /// Porcentaje entero (0 a 100) si se conoce el tamaño total.
    pub fn percent(&self) -> Option<usize> {
        self.fraction().map(|f| (f * 100.0) as usize)
    }

    pub fn is_queued(&self) -> bool {
        self.state == JobState::Queued
    }
}

/// Descargas sueltas (activas y en cola) con identificadores únicos.
#[derive(Debug)]
pub struct DownloadManager {
    jobs: Vec<DownloadJob>,
    next_id: u64,
    mode: DownloadMode,
    parallel_limit: usize,
    /// Total de trabajos de la tanda actual, incluidos los ya finalizados.
    total_jobs: usize,
    completed_jobs: usize,
    failed_jobs: usize,
    cancelled_jobs: usize,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            next_id: 1,
            mode: DownloadMode::Parallel,
            parallel_limit: MAX_PARALLEL,
            total_jobs: 0,
            completed_jobs: 0,
            failed_jobs: 0,
            cancelled_jobs: 0,
        }
    }

    /// Todos los trabajos (activos y en cola), en orden de llegada.
    pub fn jobs(&self) -> &[DownloadJob] {
        &self.jobs
    }

    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Total de la tanda actual, incluidos los trabajos que ya finalizaron.
    pub fn total_count(&self) -> usize {
        self.total_jobs
    }

    /// Número de trabajos descargados correctamente en la tanda actual.
    pub fn completed_count(&self) -> usize {
        self.completed_jobs
    }

    /// Número de trabajos que terminaron con error en la tanda actual.
    pub fn failed_count(&self) -> usize {
        self.failed_jobs
    }

    /// Número de trabajos cancelados explícitamente.
    pub fn cancelled_count(&self) -> usize {
        self.cancelled_jobs
    }

    /// Trabajos procesados (correctos + fallidos + cancelados), sin confundirlos con éxitos.
    pub fn processed_count(&self) -> usize {
        self.completed_jobs + self.failed_jobs + self.cancelled_jobs
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    pub fn active_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| j.state == JobState::Active)
            .count()
    }

    pub fn queued_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| j.state == JobState::Queued)
            .count()
    }

    /// Modo actual (vuelve a `Parallel` cuando no queda ninguna descarga).
    pub fn mode(&self) -> DownloadMode {
        self.mode
    }

    /// Cambia el modo. Las descargas ya activas no se interrumpen: el nuevo límite solo decide
    /// cuándo arranca la siguiente.
    pub fn set_mode(&mut self, mode: DownloadMode) {
        self.mode = mode;
    }

    /// Límite configurable para el modo paralelo (1..=8).
    pub fn parallel_limit(&self) -> usize {
        self.parallel_limit
    }

    pub fn set_parallel_limit(&mut self, limit: usize) {
        self.parallel_limit = limit.clamp(1, MAX_PARALLEL);
    }

    /// Límite efectivo del modo actual.
    pub fn effective_limit(&self) -> usize {
        match self.mode {
            DownloadMode::Parallel => self.parallel_limit,
            DownloadMode::Queue => 1,
        }
    }

    /// ¿Están ocupados todos los huecos de descarga del modo actual?
    pub fn is_full(&self) -> bool {
        self.active_count() >= self.effective_limit()
    }

    /// Añade una descarga a la cola y devuelve su id. No arranca nada por sí sola.
    pub fn add(&mut self, title: impl Into<String>, url: impl Into<String>) -> u64 {
        self.add_for_podcast(title, url, "Podcast")
    }

    pub fn add_for_podcast(
        &mut self,
        title: impl Into<String>,
        url: impl Into<String>,
        podcast: impl Into<String>,
    ) -> u64 {
        if self.jobs.is_empty() {
            self.total_jobs = 0;
            self.completed_jobs = 0;
            self.failed_jobs = 0;
            self.cancelled_jobs = 0;
        }
        self.total_jobs += 1;
        let id = self.next_id;
        self.next_id += 1;
        self.jobs.push(DownloadJob {
            id,
            title: title.into(),
            podcast: podcast.into(),
            url: url.into(),
            state: JobState::Queued,
            received: 0,
            size: None,
        });
        id
    }

    /// Si hay hueco, pasa a activo el trabajo en cola más antiguo y lo devuelve para lanzarlo.
    pub fn next_to_start(&mut self) -> Option<DownloadJob> {
        if self.is_full() {
            return None;
        }
        let job = self.jobs.iter_mut().find(|j| j.state == JobState::Queued)?;
        job.state = JobState::Active;
        Some(job.clone())
    }

    /// Actualiza los bytes recibidos de un trabajo (ignora ids desconocidos).
    pub fn progress(&mut self, id: u64, received: u64, size: Option<u64>) {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
            job.received = received;
            job.size = size;
        }
    }

    /// Quita un trabajo terminado y registra por separado éxito o fallo.
    pub fn finish_with_result(&mut self, id: u64, success: bool, cancelled: bool) {
        let before = self.jobs.len();
        self.jobs.retain(|j| j.id != id);
        if self.jobs.len() < before {
            if success {
                self.completed_jobs += 1;
            } else if cancelled {
                self.cancelled_jobs += 1;
            } else {
                self.failed_jobs += 1;
            }
        }
        if self.jobs.is_empty() {
            self.mode = DownloadMode::Parallel;
        }
    }

    /// Compatibilidad para los tests y usos que finalizan un trabajo exitosamente.
    pub fn finish(&mut self, id: u64) {
        self.finish_with_result(id, true, false);
    }

    /// Elimina una descarga en cola o activa del listado. El hilo activo se detiene
    /// mediante su token de cancelación, gestionado por App; cancelarla la quita del total.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.jobs.len();
        self.jobs.retain(|j| j.id != id);
        let removed = before - self.jobs.len();
        self.total_jobs = self.total_jobs.saturating_sub(removed);
        if self.jobs.is_empty() {
            self.mode = DownloadMode::Parallel;
        }
        removed > 0
    }

    /// Progreso global de la tanda: incluye trabajos finalizados y los que siguen activos/en cola.
    pub fn overall_fraction(&self) -> Option<f64> {
        (self.total_jobs > 0).then(|| {
            let current: f64 = self
                .jobs
                .iter()
                .map(|job| job.fraction().unwrap_or(0.0))
                .sum();
            ((self.processed_count() as f64 + current) / self.total_jobs as f64).clamp(0.0, 1.0)
        })
    }

    /// Porcentaje del primer trabajo activo que ya informa su tamaño.
    pub fn first_percent(&self) -> Option<usize> {
        self.jobs
            .iter()
            .filter(|j| j.state == JobState::Active)
            .find_map(|j| j.percent())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(m: &mut DownloadManager, n: usize) -> Vec<u64> {
        (0..n)
            .map(|i| m.add(format!("ep{i}"), format!("http://x/{i}.mp3")))
            .collect()
    }

    #[test]
    fn ids_are_unique_and_finish_removes_only_that_job() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 2);
        assert_ne!(ids[0], ids[1]);
        m.finish(ids[0]);
        assert_eq!(m.len(), 1);
        assert_eq!(m.jobs()[0].id, ids[1]);
    }

    #[test]
    fn add_only_queues_until_started() {
        let mut m = DownloadManager::new();
        fill(&mut m, 1);
        assert_eq!((m.active_count(), m.queued_count()), (0, 1));
        assert!(m.jobs()[0].is_queued());
    }

    #[test]
    fn never_more_than_eight_active_and_rest_wait_in_order() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 12);
        let mut started = Vec::new();
        while let Some(j) = m.next_to_start() {
            started.push(j.id);
        }
        assert_eq!(started, ids[..MAX_PARALLEL].to_vec());
        assert_eq!((m.active_count(), m.queued_count()), (8, 4));
        assert!(m.is_full());
        assert!(m.next_to_start().is_none());
        // al terminar una, arranca la siguiente en cola (la novena)
        m.finish(ids[0]);
        assert_eq!(m.next_to_start().map(|j| j.id), Some(ids[8]));
        assert_eq!((m.active_count(), m.queued_count()), (8, 3));
    }

    #[test]
    fn overall_is_mean_with_queued_counting_zero() {
        assert_eq!(overall(Vec::new()), None);
        assert_eq!(overall([Some(1.0), None]), Some(0.5));
        assert_eq!(
            overall([Some(0.5), Some(1.5), Some(0.0), None]),
            Some(0.375)
        ); // 1.5 se limita a 1.0
        let mut m = DownloadManager::new();
        assert_eq!(m.overall_fraction(), None);
        let ids = fill(&mut m, 4);
        m.next_to_start();
        m.progress(ids[0], 50, Some(100));
        assert_eq!(m.overall_fraction(), Some(0.125));
    }

    #[test]
    fn overall_progress_keeps_completed_jobs_and_resets_for_next_batch() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 3);
        m.next_to_start();
        m.progress(ids[0], 50, Some(100));
        assert_eq!(m.overall_fraction(), Some(1.0 / 6.0));
        m.finish(ids[0]);
        assert_eq!(m.overall_fraction(), Some(1.0 / 3.0));
        assert_eq!(m.total_count(), 3);
        m.finish(ids[1]);
        m.finish(ids[2]);
        assert_eq!(m.overall_fraction(), Some(1.0));
        let next = m.add("nuevo", "https://x/new");
        assert_eq!(m.total_count(), 1);
        assert_eq!(m.overall_fraction(), Some(0.0));
        assert_eq!(m.jobs()[0].id, next);
    }

    #[test]
    fn failed_jobs_are_not_counted_as_successful_downloads() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 16);
        for id in ids.iter().take(10) {
            m.finish_with_result(*id, true, false);
        }
        for id in ids.iter().skip(10) {
            m.finish_with_result(*id, false, false);
        }
        assert_eq!(m.completed_count(), 10);
        assert_eq!(m.failed_count(), 6);
        assert_eq!(m.processed_count(), 16);
        assert_eq!(m.total_count(), 16);
        assert_eq!(m.overall_fraction(), Some(1.0));
    }

    #[test]
    fn cancelled_jobs_are_processed_but_neither_successful_nor_failed() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 3);
        m.finish_with_result(ids[0], true, false);
        m.finish_with_result(ids[1], false, true);
        m.finish_with_result(ids[2], false, false);
        assert_eq!(m.completed_count(), 1);
        assert_eq!(m.cancelled_count(), 1);
        assert_eq!(m.failed_count(), 1);
        assert_eq!(m.processed_count(), 3);
        assert_eq!(m.overall_fraction(), Some(1.0));
    }

    #[test]
    fn progress_updates_percent_of_active_jobs_only() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 2);
        m.progress(ids[1], 1, Some(2)); // en cola: no cuenta
        assert_eq!(m.first_percent(), None);
        m.next_to_start();
        m.progress(ids[0], 5_000_000, Some(10_000_000));
        m.progress(999, 1, Some(1));
        assert_eq!(m.first_percent(), Some(50));
    }

    #[test]
    fn queue_mode_runs_one_at_a_time_in_order() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 3);
        m.set_mode(DownloadMode::Queue);
        assert_eq!(m.next_to_start().map(|j| j.id), Some(ids[0]));
        assert!(m.is_full() && m.next_to_start().is_none());
        assert_eq!((m.active_count(), m.queued_count()), (1, 2));
        m.finish(ids[0]);
        assert_eq!(m.next_to_start().map(|j| j.id), Some(ids[1]));
        assert_eq!((m.active_count(), m.queued_count()), (1, 1));
    }

    #[test]
    fn configurable_parallel_limit_is_clamped_and_respected() {
        let mut m = DownloadManager::new();
        m.set_parallel_limit(3);
        let ids = fill(&mut m, 7);
        let mut started = Vec::new();
        while let Some(job) = m.next_to_start() {
            started.push(job.id);
        }
        assert_eq!(started, ids[..3].to_vec());
        assert_eq!((m.active_count(), m.queued_count()), (3, 4));
        m.finish(ids[0]);
        assert_eq!(m.next_to_start().map(|job| job.id), Some(ids[3]));
        m.set_parallel_limit(99);
        assert_eq!(m.parallel_limit(), MAX_PARALLEL);
        m.set_parallel_limit(0);
        assert_eq!(m.parallel_limit(), 1);
    }

    #[test]
    fn parallel_is_default_and_mode_resets_when_empty() {
        let mut m = DownloadManager::new();
        assert_eq!(m.mode(), DownloadMode::Parallel);
        assert_eq!(
            (DownloadMode::Parallel.limit(), DownloadMode::Queue.limit()),
            (MAX_PARALLEL, 1)
        );
        let ids = fill(&mut m, 2);
        m.set_mode(DownloadMode::Queue);
        m.finish(ids[0]);
        assert_eq!(m.mode(), DownloadMode::Queue); // aún queda una
        m.finish(ids[1]);
        assert_eq!(m.mode(), DownloadMode::Parallel);
    }

    #[test]
    fn remove_queued_job_does_not_disturb_active_jobs() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 3);
        m.next_to_start();
        assert!(m.remove(ids[2]));
        assert_eq!((m.active_count(), m.queued_count()), (1, 1));
        assert_eq!(m.total_count(), 2);
        assert!(!m.remove(999));
    }

    #[test]
    fn switching_to_queue_keeps_active_jobs_and_blocks_new_starts() {
        let mut m = DownloadManager::new();
        fill(&mut m, 4);
        m.next_to_start();
        m.next_to_start();
        m.set_mode(DownloadMode::Queue);
        assert_eq!(m.active_count(), 2); // no se interrumpen
        assert!(m.is_full() && m.next_to_start().is_none());
    }

    #[test]
    fn mixed_individual_marked_and_channel_jobs_share_one_eight_job_limit() {
        // Los tres orígenes de episodios entran en el mismo DownloadManager.
        let mut m = DownloadManager::new();
        m.add("episodio individual", "https://x/individual.mp3");
        for i in 0..5 {
            m.add(
                format!("episodio marcado {i}"),
                format!("https://x/marked{i}.mp3"),
            );
        }
        for i in 0..7 {
            m.add(
                format!("episodio de canal {i}"),
                format!("https://x/channel{i}.mp3"),
            );
        }

        let mut started = Vec::new();
        while let Some(job) = m.next_to_start() {
            started.push(job.id);
            assert!(m.active_count() <= MAX_PARALLEL);
        }
        assert_eq!(started.len(), MAX_PARALLEL);
        assert_eq!((m.active_count(), m.queued_count()), (8, 5));

        // Al finalizar dos trabajos, solo se liberan dos huecos y se respeta FIFO.
        let expected = m.jobs()[MAX_PARALLEL..]
            .iter()
            .take(2)
            .map(|j| j.id)
            .collect::<Vec<_>>();
        m.finish(started[0]);
        m.finish(started[1]);
        assert_eq!(m.next_to_start().map(|j| j.id), Some(expected[0]));
        assert_eq!(m.next_to_start().map(|j| j.id), Some(expected[1]));
        assert!(m.next_to_start().is_none());
        assert_eq!(m.active_count(), MAX_PARALLEL);
    }

    #[test]
    fn queue_mode_applies_to_channel_jobs_too_and_never_starts_two() {
        let mut m = DownloadManager::new();
        m.set_mode(DownloadMode::Queue);
        let ids = fill(&mut m, 6); // lote de canal: cada episodio es un trabajo
        assert_eq!(m.next_to_start().map(|j| j.id), Some(ids[0]));
        assert_eq!((m.active_count(), m.queued_count()), (1, 5));
        assert!(m.next_to_start().is_none());

        for expected in ids.iter().skip(1) {
            m.finish(ids[ids.iter().position(|id| id == expected).unwrap() - 1]);
            assert_eq!(m.next_to_start().map(|j| j.id), Some(*expected));
            assert_eq!(m.active_count(), 1);
        }
    }

    #[test]
    fn cancelling_queued_or_active_job_does_not_break_the_queue() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 10);
        for _ in 0..MAX_PARALLEL {
            m.next_to_start();
        }
        assert!(m.remove(ids[8])); // cancelar un trabajo en espera
        assert!(m.remove(ids[0])); // retirar un activo; App envía además su token
        assert_eq!((m.active_count(), m.queued_count()), (7, 1));
        assert_eq!(m.next_to_start().map(|j| j.id), Some(ids[9]));
        assert_eq!((m.active_count(), m.queued_count()), (8, 0));
        assert!(!m.remove(ids[8]));
    }

    #[test]
    fn failed_job_is_removed_and_next_queued_job_can_start() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 9);
        for _ in 0..MAX_PARALLEL {
            m.next_to_start();
        }
        // El gestor trata éxito y error como finalización; la capa App informa el error.
        m.finish(ids[3]);
        assert_eq!(m.next_to_start().map(|j| j.id), Some(ids[8]));
        assert_eq!((m.active_count(), m.queued_count()), (8, 0));
        assert!(!m.jobs().iter().any(|job| job.id == ids[3]));
    }

    #[test]
    fn parallel_limit_is_preserved_after_repeated_finish_and_refill_cycles() {
        let mut m = DownloadManager::new();
        let ids = fill(&mut m, 25);
        let mut next = 0usize;
        while let Some(job) = m.next_to_start() {
            assert!(m.active_count() <= MAX_PARALLEL);
            next += 1;
            if next % 3 == 0 {
                m.finish(job.id);
            }
        }
        while let Some(active_id) = m
            .jobs()
            .iter()
            .find(|j| j.state == JobState::Active)
            .map(|j| j.id)
        {
            assert!(m.active_count() <= MAX_PARALLEL);
            m.finish(active_id);
            if let Some(job) = m.next_to_start() {
                assert!(m.active_count() <= MAX_PARALLEL);
                assert!(ids.contains(&job.id));
            }
        }
        assert!(m.is_empty());
    }
}
