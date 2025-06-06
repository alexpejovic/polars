use arrow::offset::Offsets;

use super::*;
use crate::chunked_array::builder::ListNullChunkedBuilder;
use crate::series::implementations::null::NullChunked;

pub trait AggList {
    /// # Safety
    ///
    /// groups should be in bounds
    unsafe fn agg_list(&self, _groups: &GroupsType) -> Series;
}

impl<T: PolarsNumericType> AggList for ChunkedArray<T> {
    unsafe fn agg_list(&self, groups: &GroupsType) -> Series {
        let ca = self.rechunk();

        match groups {
            GroupsType::Idx(groups) => {
                let mut can_fast_explode = true;

                let arr = ca.downcast_iter().next().unwrap();
                let values = arr.values();

                let mut offsets = Vec::<i64>::with_capacity(groups.len() + 1);
                let mut length_so_far = 0i64;
                offsets.push(length_so_far);

                let mut list_values = Vec::<T::Native>::with_capacity(self.len());
                groups.iter().for_each(|(_, idx)| {
                    let idx_len = idx.len();
                    if idx_len == 0 {
                        can_fast_explode = false;
                    }

                    length_so_far += idx_len as i64;
                    // SAFETY:
                    // group tuples are in bounds
                    {
                        list_values.extend(idx.iter().map(|idx| {
                            debug_assert!((*idx as usize) < values.len());
                            *values.get_unchecked(*idx as usize)
                        }));
                        // SAFETY:
                        // we know that offsets has allocated enough slots
                        offsets.push_unchecked(length_so_far);
                    }
                });

                let validity = if arr.null_count() > 0 {
                    let old_validity = arr.validity().unwrap();
                    let mut validity = MutableBitmap::from_len_set(list_values.len());

                    let mut count = 0;
                    groups.iter().for_each(|(_, idx)| {
                        for i in idx.as_slice() {
                            if !old_validity.get_bit_unchecked(*i as usize) {
                                validity.set_unchecked(count, false);
                            }
                            count += 1;
                        }
                    });
                    Some(validity.into())
                } else {
                    None
                };

                let array = PrimitiveArray::new(
                    T::get_static_dtype().to_arrow(CompatLevel::newest()),
                    list_values.into(),
                    validity,
                );
                let dtype = ListArray::<i64>::default_datatype(
                    T::get_static_dtype().to_arrow(CompatLevel::newest()),
                );
                // SAFETY:
                // offsets are monotonically increasing
                let arr = ListArray::<i64>::new(
                    dtype,
                    Offsets::new_unchecked(offsets).into(),
                    Box::new(array),
                    None,
                );

                let mut ca = ListChunked::with_chunk(self.name().clone(), arr);
                if can_fast_explode {
                    ca.set_fast_explode()
                }
                ca.into()
            },
            GroupsType::Slice { groups, .. } => {
                let mut can_fast_explode = true;
                let arr = ca.downcast_iter().next().unwrap();
                let values = arr.values();

                let mut offsets = Vec::<i64>::with_capacity(groups.len() + 1);
                let mut length_so_far = 0i64;
                offsets.push(length_so_far);

                let mut list_values = Vec::<T::Native>::with_capacity(self.len());
                groups.iter().for_each(|&[first, len]| {
                    if len == 0 {
                        can_fast_explode = false;
                    }

                    length_so_far += len as i64;
                    list_values.extend_from_slice(&values[first as usize..(first + len) as usize]);
                    {
                        // SAFETY:
                        // we know that offsets has allocated enough slots
                        offsets.push_unchecked(length_so_far);
                    }
                });

                let validity = if arr.null_count() > 0 {
                    let old_validity = arr.validity().unwrap();
                    let mut validity = MutableBitmap::from_len_set(list_values.len());

                    let mut count = 0;
                    groups.iter().for_each(|[first, len]| {
                        for i in *first..(*first + *len) {
                            if !old_validity.get_bit_unchecked(i as usize) {
                                validity.set_unchecked(count, false)
                            }
                            count += 1;
                        }
                    });
                    Some(validity.into())
                } else {
                    None
                };

                let array = PrimitiveArray::new(
                    T::get_static_dtype().to_arrow(CompatLevel::newest()),
                    list_values.into(),
                    validity,
                );
                let dtype = ListArray::<i64>::default_datatype(
                    T::get_static_dtype().to_arrow(CompatLevel::newest()),
                );
                let arr = ListArray::<i64>::new(
                    dtype,
                    Offsets::new_unchecked(offsets).into(),
                    Box::new(array),
                    None,
                );
                let mut ca = ListChunked::with_chunk(self.name().clone(), arr);
                if can_fast_explode {
                    ca.set_fast_explode()
                }
                ca.into()
            },
        }
    }
}

impl AggList for NullChunked {
    unsafe fn agg_list(&self, groups: &GroupsType) -> Series {
        match groups {
            GroupsType::Idx(groups) => {
                let mut builder = ListNullChunkedBuilder::new(self.name().clone(), groups.len());
                for idx in groups.all().iter() {
                    builder.append_with_len(idx.len());
                }
                builder.finish().into_series()
            },
            GroupsType::Slice { groups, .. } => {
                let mut builder = ListNullChunkedBuilder::new(self.name().clone(), groups.len());
                for [_, len] in groups {
                    builder.append_with_len(*len as usize);
                }
                builder.finish().into_series()
            },
        }
    }
}

impl AggList for BooleanChunked {
    unsafe fn agg_list(&self, groups: &GroupsType) -> Series {
        agg_list_by_gather_and_offsets(self, groups)
    }
}

impl AggList for StringChunked {
    unsafe fn agg_list(&self, groups: &GroupsType) -> Series {
        agg_list_by_gather_and_offsets(self, groups)
    }
}

impl AggList for BinaryChunked {
    unsafe fn agg_list(&self, groups: &GroupsType) -> Series {
        agg_list_by_gather_and_offsets(self, groups)
    }
}

impl AggList for ListChunked {
    unsafe fn agg_list(&self, groups: &GroupsType) -> Series {
        agg_list_by_gather_and_offsets(self, groups)
    }
}

#[cfg(feature = "dtype-array")]
impl AggList for ArrayChunked {
    unsafe fn agg_list(&self, groups: &GroupsType) -> Series {
        agg_list_by_gather_and_offsets(self, groups)
    }
}

#[cfg(feature = "object")]
impl<T: PolarsObject> AggList for ObjectChunked<T> {
    unsafe fn agg_list(&self, groups: &GroupsType) -> Series {
        let mut can_fast_explode = true;
        let mut offsets = Vec::<i64>::with_capacity(groups.len() + 1);
        let mut length_so_far = 0i64;
        offsets.push(length_so_far);

        //  we know that iterators length
        let iter = {
            groups
                .iter()
                .flat_map(|indicator| {
                    let (group_vals, len) = match indicator {
                        GroupsIndicator::Idx((_first, idx)) => {
                            // SAFETY:
                            // group tuples always in bounds
                            let group_vals = self.take_unchecked(idx);

                            (group_vals, idx.len() as IdxSize)
                        },
                        GroupsIndicator::Slice([first, len]) => {
                            let group_vals = _slice_from_offsets(self, first, len);

                            (group_vals, len)
                        },
                    };

                    if len == 0 {
                        can_fast_explode = false;
                    }
                    length_so_far += len as i64;
                    // SAFETY:
                    // we know that offsets has allocated enough slots
                    offsets.push_unchecked(length_so_far);

                    let arr = group_vals.downcast_iter().next().unwrap().clone();
                    arr.into_iter_cloned()
                })
                .trust_my_length(self.len())
        };

        let mut pe = create_extension(iter);

        // SAFETY: This is safe because we just created the PolarsExtension
        // meaning that the sentinel is heap allocated and the dereference of
        // the pointer does not fail.
        pe.set_to_series_fn::<T>();
        let extension_array = Box::new(pe.take_and_forget()) as ArrayRef;
        let extension_dtype = extension_array.dtype();

        let dtype = ListArray::<i64>::default_datatype(extension_dtype.clone());
        // SAFETY: offsets are monotonically increasing.
        let arr = ListArray::<i64>::new(
            dtype,
            Offsets::new_unchecked(offsets).into(),
            extension_array,
            None,
        );
        let mut listarr = ListChunked::with_chunk(self.name().clone(), arr);
        if can_fast_explode {
            listarr.set_fast_explode()
        }
        listarr.into_series()
    }
}

#[cfg(feature = "dtype-struct")]
impl AggList for StructChunked {
    unsafe fn agg_list(&self, groups: &GroupsType) -> Series {
        let ca = self.clone();
        let (gather, offsets, can_fast_explode) = groups.prepare_list_agg(self.len());

        let gathered = if let Some(gather) = gather {
            let out = ca.into_series().take_unchecked(&gather);
            out.struct_().unwrap().clone()
        } else {
            ca.rechunk().into_owned()
        };

        let arr = gathered.chunks()[0].clone();
        let dtype = LargeListArray::default_datatype(arr.dtype().clone());

        let mut chunk = ListChunked::with_chunk(
            self.name().clone(),
            LargeListArray::new(dtype, offsets, arr, None),
        );
        chunk.set_dtype(DataType::List(Box::new(self.dtype().clone())));
        if can_fast_explode {
            chunk.set_fast_explode()
        }

        chunk.into_series()
    }
}

unsafe fn agg_list_by_gather_and_offsets<T: PolarsDataType>(
    ca: &ChunkedArray<T>,
    groups: &GroupsType,
) -> Series
where
    ChunkedArray<T>: ChunkTakeUnchecked<IdxCa>,
{
    let (gather, offsets, can_fast_explode) = groups.prepare_list_agg(ca.len());

    let gathered = if let Some(gather) = gather {
        ca.take_unchecked(&gather)
    } else {
        ca.clone()
    };

    let arr = gathered.chunks()[0].clone();
    let dtype = LargeListArray::default_datatype(arr.dtype().clone());

    let mut chunk = ListChunked::with_chunk(
        ca.name().clone(),
        LargeListArray::new(dtype, offsets, arr, None),
    );
    chunk.set_dtype(DataType::List(Box::new(ca.dtype().clone())));
    if can_fast_explode {
        chunk.set_fast_explode()
    }

    chunk.into_series()
}
