      program ref
      implicit none
      integer kw,i,j,k
      INTEGER ceflag,tflag,ifflag,nsflag,wdflag
      COMMON /FLAGS/ ceflag,tflag,ifflag,nsflag,wdflag
      REAL*8 neta,bwind,hewind,mxns
      COMMON /VALUE1/ neta,bwind,hewind,mxns
      real*8 z,zpars(20),mass,mt,aj,tm,tn,tscls(20),lums(10),GB(10)
      real*8 r,lum,mc,rc,menv,renv,k2
      real*8 zs(3),ms(12),fr(6)
      data zs /0.02d0,0.001d0,0.03d0/
      data ms /0.08d0,0.1d0,0.2d0,0.5d0,0.69d0,0.8d0,1.0d0,1.1d0,
     &         1.2d0,1.5d0,2.0d0,5.0d0/
      data fr /0.0d0,0.25d0,0.5d0,0.9d0,0.995d0,1.0d0/
      ceflag = 0
      tflag = 1
      ifflag = 0
      nsflag = 1
      wdflag = 1
      neta = 0.5d0
      bwind = 0.d0
      hewind = 0.5d0
      mxns = 3.d0
      do i = 1,3
         z = zs(i)
         call zcnsts(z,zpars)
         do j = 1,12
            do k = 1,6
               mass = ms(j)
               mt = mass
               kw = 1
               call star(kw,mass,mt,tm,tn,tscls,lums,GB,zpars)
               aj = fr(k)*tm
               if(k.eq.6) aj = 0.999999d0*tm
               call hrdiag(mass,aj,mt,tm,tn,tscls,lums,GB,zpars,
     &                     r,lum,kw,mc,rc,menv,renv,k2)
               write(*,'(f6.3,1x,f6.4,1x,es24.16,1x,es24.16,1x,
     &               es24.16,1x,es24.16,1x,i2)') ms(j),z,aj,tm,lum,r,kw
            enddo
         enddo
      enddo
*     the Sun today
      z = 0.02d0
      call zcnsts(z,zpars)
      mass = 1.d0
      mt = 1.d0
      kw = 1
      call star(kw,mass,mt,tm,tn,tscls,lums,GB,zpars)
      aj = 4570.d0
      call hrdiag(mass,aj,mt,tm,tn,tscls,lums,GB,zpars,
     &            r,lum,kw,mc,rc,menv,renv,k2)
      write(*,'(f6.3,1x,f6.4,1x,es24.16,1x,es24.16,1x,
     &      es24.16,1x,es24.16,1x,i2)') mass,z,aj,tm,lum,r,kw
      end
