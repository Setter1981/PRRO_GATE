using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Timers;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormTimer : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("TextC")]
	private TextBox _TextC;

	[CompilerGenerated]
	[AccessedThroughProperty("Indi")]
	private PictureBox _Indi;

	[CompilerGenerated]
	[AccessedThroughProperty("IndiT")]
	private PictureBox _IndiT;

	[CompilerGenerated]
	[AccessedThroughProperty("IndiK")]
	private PictureBox _IndiK;

	[CompilerGenerated]
	[AccessedThroughProperty("VisB")]
	private PictureBox _VisB;

	[CompilerGenerated]
	[AccessedThroughProperty("Timer1")]
	private Timer _Timer1;

	private StreamWriter myWriteFile;

	private string FNf;

	private bool FullV;

	private long tc;

	private const int EndC = 3;

	private int EndT;

	private bool Vis;

	private const int VisTrue = -3;

	private int nIndex;

	private Dispatch DS;

	private bool AlertEnd;

	internal virtual TextBox TextC
	{
		[CompilerGenerated]
		get
		{
			return _TextC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = TextC_MouseHover;
			TextBox textC = _TextC;
			if (textC != null)
			{
				((Control)textC).MouseHover -= eventHandler;
			}
			_TextC = value;
			textC = _TextC;
			if (textC != null)
			{
				((Control)textC).MouseHover += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("OfflineCount")]
	internal virtual TextBox OfflineCount
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual PictureBox Indi
	{
		[CompilerGenerated]
		get
		{
			return _Indi;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Indi_Click;
			EventHandler eventHandler2 = Indi_MouseHover;
			PictureBox indi = _Indi;
			if (indi != null)
			{
				((Control)indi).Click -= eventHandler;
				((Control)indi).MouseHover -= eventHandler2;
			}
			_Indi = value;
			indi = _Indi;
			if (indi != null)
			{
				((Control)indi).Click += eventHandler;
				((Control)indi).MouseHover += eventHandler2;
			}
		}
	}

	internal virtual PictureBox IndiT
	{
		[CompilerGenerated]
		get
		{
			return _IndiT;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = IndiT_Click;
			EventHandler eventHandler2 = IndiT_MouseHover;
			PictureBox indiT = _IndiT;
			if (indiT != null)
			{
				((Control)indiT).Click -= eventHandler;
				((Control)indiT).MouseHover -= eventHandler2;
			}
			_IndiT = value;
			indiT = _IndiT;
			if (indiT != null)
			{
				((Control)indiT).Click += eventHandler;
				((Control)indiT).MouseHover += eventHandler2;
			}
		}
	}

	internal virtual PictureBox IndiK
	{
		[CompilerGenerated]
		get
		{
			return _IndiK;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = IndiK_Click;
			EventHandler eventHandler2 = IndiK_MouseHover;
			PictureBox indiK = _IndiK;
			if (indiK != null)
			{
				((Control)indiK).Click -= eventHandler;
				((Control)indiK).MouseHover -= eventHandler2;
			}
			_IndiK = value;
			indiK = _IndiK;
			if (indiK != null)
			{
				((Control)indiK).Click += eventHandler;
				((Control)indiK).MouseHover += eventHandler2;
			}
		}
	}

	internal virtual PictureBox VisB
	{
		[CompilerGenerated]
		get
		{
			return _VisB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = VisB_Click;
			EventHandler eventHandler2 = VisB_MouseHover;
			PictureBox visB = _VisB;
			if (visB != null)
			{
				((Control)visB).Click -= eventHandler;
				((Control)visB).MouseHover -= eventHandler2;
			}
			_VisB = value;
			visB = _VisB;
			if (visB != null)
			{
				((Control)visB).Click += eventHandler;
				((Control)visB).MouseHover += eventHandler2;
			}
		}
	}

	[field: AccessedThroughProperty("ToolTip1")]
	internal virtual ToolTip ToolTip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	private virtual Timer Timer1
	{
		[CompilerGenerated]
		get
		{
			return _Timer1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			ElapsedEventHandler value2 = Timer1_Tick;
			Timer timer = _Timer1;
			if (timer != null)
			{
				timer.Elapsed -= value2;
			}
			_Timer1 = value;
			timer = _Timer1;
			if (timer != null)
			{
				timer.Elapsed += value2;
			}
		}
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_0064: Unknown result type (might be due to invalid IL or missing references)
		//IL_006e: Expected O, but got Unknown
		//IL_00cf: Unknown result type (might be due to invalid IL or missing references)
		//IL_00d9: Expected O, but got Unknown
		//IL_0156: Unknown result type (might be due to invalid IL or missing references)
		//IL_0160: Expected O, but got Unknown
		//IL_03f6: Unknown result type (might be due to invalid IL or missing references)
		//IL_0400: Expected O, but got Unknown
		components = new Container();
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormTimer));
		TextC = new TextBox();
		OfflineCount = new TextBox();
		Indi = new PictureBox();
		IndiT = new PictureBox();
		IndiK = new PictureBox();
		VisB = new PictureBox();
		ToolTip1 = new ToolTip(components);
		((ISupportInitialize)Indi).BeginInit();
		((ISupportInitialize)IndiT).BeginInit();
		((ISupportInitialize)IndiK).BeginInit();
		((ISupportInitialize)VisB).BeginInit();
		((Control)this).SuspendLayout();
		((TextBoxBase)TextC).BorderStyle = (BorderStyle)0;
		((Control)TextC).Enabled = false;
		((Control)TextC).Font = new Font("Microsoft Sans Serif", 7.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextC).Location = new Point(2, 2);
		((Control)TextC).Name = "TextC";
		((Control)TextC).Size = new Size(105, 15);
		((Control)TextC).TabIndex = 0;
		TextC.TextAlign = (HorizontalAlignment)2;
		((TextBoxBase)OfflineCount).BorderStyle = (BorderStyle)0;
		((Control)OfflineCount).Enabled = false;
		((Control)OfflineCount).Font = new Font("Microsoft Sans Serif", 7.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OfflineCount).Location = new Point(2, 39);
		((Control)OfflineCount).Name = "OfflineCount";
		((Control)OfflineCount).Size = new Size(105, 15);
		((Control)OfflineCount).TabIndex = 2;
		OfflineCount.TextAlign = (HorizontalAlignment)2;
		Indi.BorderStyle = (BorderStyle)2;
		((Control)Indi).Location = new Point(2, 23);
		((Control)Indi).Name = "Indi";
		((Control)Indi).Size = new Size(31, 10);
		Indi.TabIndex = 3;
		Indi.TabStop = false;
		IndiT.BorderStyle = (BorderStyle)2;
		((Control)IndiT).Location = new Point(39, 23);
		((Control)IndiT).Name = "IndiT";
		((Control)IndiT).Size = new Size(31, 10);
		IndiT.TabIndex = 4;
		IndiT.TabStop = false;
		IndiK.BorderStyle = (BorderStyle)2;
		((Control)IndiK).Location = new Point(76, 23);
		((Control)IndiK).Name = "IndiK";
		((Control)IndiK).Size = new Size(31, 10);
		IndiK.TabIndex = 5;
		IndiK.TabStop = false;
		((Control)VisB).BackColor = Color.FromArgb(128, 255, 128);
		VisB.BorderStyle = (BorderStyle)1;
		((Control)VisB).Location = new Point(113, 2);
		((Control)VisB).Name = "VisB";
		((Control)VisB).Size = new Size(20, 57);
		VisB.SizeMode = (PictureBoxSizeMode)4;
		VisB.TabIndex = 6;
		VisB.TabStop = false;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(155, 59);
		((Form)this).ControlBox = false;
		((Control)this).Controls.Add((Control)(object)VisB);
		((Control)this).Controls.Add((Control)(object)IndiK);
		((Control)this).Controls.Add((Control)(object)IndiT);
		((Control)this).Controls.Add((Control)(object)Indi);
		((Control)this).Controls.Add((Control)(object)OfflineCount);
		((Control)this).Controls.Add((Control)(object)TextC);
		((Form)this).FormBorderStyle = (FormBorderStyle)0;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormTimer";
		((Form)this).Text = "Счетчик";
		((Form)this).TopMost = true;
		((ISupportInitialize)Indi).EndInit();
		((ISupportInitialize)IndiT).EndInit();
		((ISupportInitialize)IndiK).EndInit();
		((ISupportInitialize)VisB).EndInit();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormTimer(string fn)
	{
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		((Form)this).Load += FormTimer_Load;
		((Form)this).Closed += FormTimer_Closed;
		((Control)this).MouseClick += new MouseEventHandler(FormTimer_MouseClick);
		((Form)this).Closing += FormTimer_Closing;
		((Control)this).Resize += FormTimer_Resize;
		((Component)this).Disposed += FormTimer_Disposed;
		Timer1 = new Timer();
		tc = 0L;
		EndT = 3;
		Vis = true;
		AlertEnd = false;
		InitializeComponent();
		FNf = fn;
	}

	~FormTimer()
	{
		((Component)this).Finalize();
	}

	internal TypErr StartControl()
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		string path = All.MyDoc() + "\\WebCheck\\Temp\\All\\" + FNf + ".wcf";
		try
		{
			myWriteFile = new StreamWriter(path, append: false);
			myWriteFile.WriteLine(FNf);
			Application.DoEvents();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 52;
			result.errStr = "Контрольная форма для этого фискального номера уже загружена";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal bool StopControl()
	{
		try
		{
			myWriteFile.Flush();
			myWriteFile.Dispose();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		return KilFile();
	}

	private bool KilFile()
	{
		bool result;
		try
		{
			string path = All.MyDoc() + "\\WebCheck\\Temp\\All\\" + FNf + ".wcf";
			if (File.Exists(path))
			{
				File.Delete(path);
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_003d;
		}
		result = true;
		goto IL_003d;
		IL_003d:
		return result;
	}

	private void FormTimer_Load(object sender, EventArgs e)
	{
		((Control)this).Left = -1000;
		if (All.f.IntegerGetFn(All.A.FN, "ShowInTaskbar") != 0)
		{
			All.f.IntigerWriteFN(All.A.FN, "ShowInTaskbar", 1);
		}
		else
		{
			All.f.IntigerWriteFN(All.A.FN, "ShowInTaskbar", 0);
			((Form)this).ShowInTaskbar = false;
		}
		((Control)Indi).BackColor = Color.LightGreen;
		if (All.A.TypWork == 2020)
		{
			((Control)IndiK).BackColor = Color.Green;
		}
		else
		{
			((Control)IndiK).BackColor = Color.LightGreen;
		}
		((Control)IndiT).BackColor = Color.LightGreen;
		((Control)VisB).BackColor = Color.LightGreen;
		checked
		{
			((Control)this).Width = 2 * ((Control)OfflineCount).Left + ((Control)OfflineCount).Width;
			((Control)this).Height = 2 * ((Control)OfflineCount).Left + ((Control)OfflineCount).Top + ((Control)OfflineCount).Height;
			if (StartControl().errCode > 0)
			{
				Timer1.Interval = 900.0;
				Timer1.Start();
				AlertEnd = true;
				((Component)this).Dispose();
				return;
			}
			new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + FNf + "\\bl.ini").IntigerWriteFN(FNf, "Block", 0);
			((Form)this).Text = "ПРРО " + FNf;
			tc = 0L;
			if ((All.A.TypWork == 2020) & (All.A.PullY > 0))
			{
				((Control)this).Top = All.lC.Y(All.A.PullY);
			}
			else
			{
				nIndex = All.lC.YGet(FNf);
				((Control)this).Top = All.lC.Y(nIndex);
			}
			((Control)this).Left = -3;
			Vis = true;
			OperatorsAll operatorsAll = new OperatorsAll();
			Coding coding = new Coding();
			FullV = All.A.FullVersion;
			DS = new Dispatch(FNf, All.A.Connection, operatorsAll.get_Seller(2, 1), coding.DeCod(operatorsAll.get_Seller(3, 1)), All.A.AcskSettings, All.Lg.PathFile, All.A.FiscalMode, All.A.TIN);
			if (Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", false) == 0)
			{
				if (Environment.UserInteractive)
				{
					TextC.Text = FNf + " TS";
				}
				((Form)this).BackColor = Color.White;
				((TextBoxBase)TextC).BackColor = Color.White;
				((TextBoxBase)OfflineCount).BackColor = Color.White;
				((TextBoxBase)TextC).ForeColor = Color.Black;
				((TextBoxBase)OfflineCount).ForeColor = Color.Black;
			}
			else
			{
				if (Environment.UserInteractive)
				{
					TextC.Text = FNf;
				}
				((TextBoxBase)TextC).ForeColor = Color.Black;
				((TextBoxBase)OfflineCount).ForeColor = Color.Black;
			}
			if (FullV)
			{
				int e2 = new NumbersOfflineUseRobot(DS.Connection, DS.FN).CountNubmers();
				if (Environment.UserInteractive)
				{
					OfflineCount.Text = e2 + " / " + DS.OfflineCheckCount();
				}
				IndiOfflineCount(e2);
			}
			else if (Operators.CompareString(FNf, "7000000512", false) == 0)
			{
				if (Environment.UserInteractive)
				{
					OfflineCount.Text = "DEMO";
				}
			}
			else if (Environment.UserInteractive)
			{
				OfflineCount.Text = "FREE";
			}
			if (Environment.UserInteractive && Operators.CompareString(FNf, "7000000512", false) == 0)
			{
				OfflineCount.Text = "DEMO";
			}
			Application.DoEvents();
			((Control)this).Show();
			((Form)this).WindowState = (FormWindowState)0;
			Application.DoEvents();
			if (All.A.TypWork == 2020)
			{
				DS.OreratorKeyDataEndIni();
				Timer1.Interval = 333.0;
				Timer1.Start();
			}
			else
			{
				Timer1.Interval = 9000.0;
				Timer1.Start();
			}
			All.A.FormRobot = true;
		}
	}

	private void Timer1_Tick(object sender, EventArgs e)
	{
		if (AlertEnd)
		{
			Timer1.Stop();
			((Component)this).Dispose();
			return;
		}
		if (Operators.CompareString(FNf, "7000000512", false) == 0)
		{
			((Control)Indi).BackColor = Color.LightGreen;
			((Control)VisB).BackColor = ((Control)Indi).BackColor;
			if (Environment.UserInteractive)
			{
				OfflineCount.Text = "DEMO";
			}
			if (All.A.TypWork == 2020)
			{
				AlertEnd = true;
				((Component)this).Dispose();
			}
			return;
		}
		if (!FullV)
		{
			((Control)Indi).BackColor = Color.LightGreen;
			((Control)VisB).BackColor = ((Control)Indi).BackColor;
			if (Environment.UserInteractive)
			{
				OfflineCount.Text = "FREE";
			}
			if (All.A.TypWork == 2020)
			{
				AlertEnd = true;
				((Component)this).Dispose();
			}
			return;
		}
		if (!All.SecondRunControlR())
		{
			AlertEnd = true;
			((Component)this).Dispose();
			return;
		}
		Timer1.Stop();
		checked
		{
			tc++;
			NumbersOfflineUseRobot numbersOfflineUseRobot = new NumbersOfflineUseRobot(DS.Connection, DS.FN);
			int num = numbersOfflineUseRobot.CountNubmers();
			if (FullV)
			{
				if (Environment.UserInteractive)
				{
					OfflineCount.Text = num + " / " + DS.OfflineCheckCount();
				}
			}
			else if (Operators.CompareString(FNf, "7000000512", false) == 0)
			{
				if (Environment.UserInteractive)
				{
					OfflineCount.Text = "DEMO";
				}
			}
			else if (Environment.UserInteractive)
			{
				OfflineCount.Text = "FREE";
			}
			Application.DoEvents();
			if (DS.OfflineTrue())
			{
				Application.DoEvents();
				DateTime now = DateTime.Now;
				int num2 = DS.OfflineToOnline();
				DateTime now2 = DateTime.Now;
				if (All.A.TypWork == 2020)
				{
					if (num2 == 0)
					{
						EndT = 3;
					}
					else if (num2 > 0)
					{
						EndT = 3;
					}
					else if (num2 < 0)
					{
						EndT--;
						if (EndT < 1)
						{
							All.Lg.SaveTextToLog("Robot Server", "Ошибка повторилась более 9 раз");
							AlertEnd = true;
							((Component)this).Dispose();
						}
					}
				}
				switch (num2)
				{
				case -3:
					if (All.A.TypWork == 2020)
					{
						Timer1.Interval = 27000.0;
						((Control)Indi).BackColor = Color.DarkRed;
					}
					else
					{
						Timer1.Interval = 1080000.0;
						((Control)Indi).BackColor = Color.DarkRed;
					}
					break;
				case -2:
					if (All.A.TypWork == 2020)
					{
						Timer1.Interval = 18000.0;
						((Control)Indi).BackColor = Color.Red;
					}
					else
					{
						Timer1.Interval = 270000.0;
						((Control)Indi).BackColor = Color.Red;
					}
					break;
				case -1:
					if (All.A.TypWork == 2020)
					{
						Timer1.Interval = 18000.0;
						((Control)Indi).BackColor = Color.Yellow;
					}
					else
					{
						Timer1.Interval = 270000.0;
						((Control)Indi).BackColor = Color.Yellow;
					}
					break;
				case 0:
					if (All.A.TypWork == 2020)
					{
						Timer1.Interval = 3000.0;
						((Control)Indi).BackColor = Color.LightGreen;
					}
					else
					{
						Timer1.Interval = 540000.0;
						((Control)Indi).BackColor = Color.LightGreen;
					}
					break;
				case 1:
					if (All.A.TypWork == 2020)
					{
						Timer1.Interval = 2000.0;
						((Control)Indi).BackColor = Color.Green;
					}
					else
					{
						Timer1.Interval = 9000.0;
						((Control)Indi).BackColor = Color.Green;
					}
					break;
				}
				if (num2 < 0 && (int)Math.Abs(DateAndTime.DateDiff((DateInterval)9, now2, now, (FirstDayOfWeek)1, (FirstWeekOfYear)1)) > 21)
				{
					Timer1.Interval = 1620000.0;
					((Control)Indi).BackColor = Color.PaleVioletRed;
					if (All.A.TypWork == 2020)
					{
						Timer1.Interval = 9000.0;
						All.Lg.SaveTextToLog("Robot Server", "Ожидание ответа более 21 секунды");
						AlertEnd = true;
						((Component)this).Dispose();
					}
				}
			}
			else
			{
				((Control)Indi).BackColor = Color.LightGreen;
				Timer1.Interval = 90000.0;
				TypErr offlineNumberAutomatic = DS.GetOfflineNumberAutomatic();
				if (All.A.TypWork == 2020)
				{
					Timer1.Interval = 3000.0;
					if (offlineNumberAutomatic.errCode == 0)
					{
						EndT = 0;
					}
					else
					{
						EndT--;
					}
					if (EndT < 1)
					{
						AlertEnd = true;
						((Component)this).Dispose();
					}
				}
				else
				{
					DS.OreratorKeyDataEndIni();
				}
			}
			((Control)VisB).BackColor = ((Control)Indi).BackColor;
			num = numbersOfflineUseRobot.CountNubmers();
			if (Environment.UserInteractive)
			{
				OfflineCount.Text = num + " / " + DS.OfflineCheckCount();
			}
			if (All.A.TypWork == 2020)
			{
				switch (EndT)
				{
				case 3:
					((Control)IndiK).BackColor = Color.Green;
					break;
				case 2:
					((Control)IndiK).BackColor = Color.Yellow;
					break;
				case 1:
					((Control)IndiK).BackColor = Color.Pink;
					break;
				case 0:
					((Control)IndiK).BackColor = Color.Red;
					break;
				}
			}
			else
			{
				((Control)IndiK).BackColor = Color.LightGreen;
			}
			IndiOfflineCount(num);
			Application.DoEvents();
			Timer1.Start();
		}
	}

	private void IndiOfflineCount(int e)
	{
		if (e > 999)
		{
			((Control)IndiT).BackColor = Color.LightGreen;
		}
		else if (e > 499)
		{
			((Control)IndiT).BackColor = Color.GreenYellow;
		}
		else if (e > 199)
		{
			((Control)IndiT).BackColor = Color.YellowGreen;
		}
		else if (e > 99)
		{
			((Control)IndiT).BackColor = Color.LightPink;
		}
		else if (e > 49)
		{
			((Control)IndiT).BackColor = Color.LightCoral;
		}
		else if (e > 3)
		{
			((Control)IndiT).BackColor = Color.Coral;
		}
		else
		{
			((Control)IndiT).BackColor = Color.Red;
		}
	}

	private void FormTimer_Closed(object sender, EventArgs e)
	{
	}

	private void FormTimer_MouseClick(object sender, MouseEventArgs e)
	{
		VisPr();
	}

	private void VisB_Click(object sender, EventArgs e)
	{
		VisPr();
	}

	private void VisPr()
	{
		checked
		{
			if (Vis)
			{
				((Control)this).Left = ((Control)VisB).Left * -1;
				Vis = false;
				((Control)this).Width = ((Control)VisB).Left + ((Control)VisB).Width + ((Control)OfflineCount).Left;
			}
			else
			{
				((Control)this).Left = -3;
				Vis = true;
				((Control)this).Width = 2 * ((Control)OfflineCount).Left + ((Control)OfflineCount).Width;
			}
		}
	}

	private void IndiK_Click(object sender, EventArgs e)
	{
		VisPr();
	}

	private void IndiT_Click(object sender, EventArgs e)
	{
		VisPr();
	}

	private void Indi_Click(object sender, EventArgs e)
	{
		VisPr();
	}

	private void VisB_MouseHover(object sender, EventArgs e)
	{
		//IL_0007: Unknown result type (might be due to invalid IL or missing references)
		//IL_0021: Expected O, but got Unknown
		ToolTip1.SetToolTip((Control)sender, "ПРРО " + FNf);
	}

	private void TextC_MouseHover(object sender, EventArgs e)
	{
		//IL_0007: Unknown result type (might be due to invalid IL or missing references)
		//IL_0021: Expected O, but got Unknown
		ToolTip1.SetToolTip((Control)sender, "ПРРО " + FNf);
	}

	private void Indi_MouseHover(object sender, EventArgs e)
	{
		//IL_0007: Unknown result type (might be due to invalid IL or missing references)
		//IL_0016: Expected O, but got Unknown
		ToolTip1.SetToolTip((Control)sender, "Индикатор офлайн режима");
	}

	private void IndiT_MouseHover(object sender, EventArgs e)
	{
		//IL_0007: Unknown result type (might be due to invalid IL or missing references)
		//IL_0016: Expected O, but got Unknown
		ToolTip1.SetToolTip((Control)sender, "Индикатор контроля времени работы");
	}

	private void IndiK_MouseHover(object sender, EventArgs e)
	{
		//IL_0007: Unknown result type (might be due to invalid IL or missing references)
		//IL_0016: Expected O, but got Unknown
		ToolTip1.SetToolTip((Control)sender, "Индикатор состояния ключей и сертификатов");
	}

	private void FormTimer_Closing(object sender, CancelEventArgs e)
	{
		//IL_0039: Unknown result type (might be due to invalid IL or missing references)
		//IL_003f: Invalid comparison between Unknown and I4
		if ((All.A.TypWork == 2020) | (All.A.TypWork == 2019))
		{
			e.Cancel = false;
		}
		else if ((int)Interaction.MsgBox((object)"УВАГА!   Закриття форми призведе до припинення відправки офлайн чеків. Закрити форму?", (MsgBoxStyle)33, (object)"Контроль роботи офлайн!") == 1)
		{
			e.Cancel = false;
		}
		else
		{
			e.Cancel = true;
		}
	}

	private void FormTimer_Resize(object sender, EventArgs e)
	{
		((Form)this).WindowState = (FormWindowState)0;
	}

	private void FormTimer_Disposed(object sender, EventArgs e)
	{
		Timer1.Stop();
		int num = 0;
		while (!StopControl())
		{
			num = checked(num + 1);
			if (num > 108)
			{
				break;
			}
		}
		All.A.FormRobot = false;
		All.lC.YClear(nIndex, FNf);
		new IniHGB(All.MyDoc() + "\\WebCheck\\settings.ini").IntigerWriteFN(FNf, "Block", 0);
	}
}
