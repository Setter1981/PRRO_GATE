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
	private System.Timers.Timer _Timer1;

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
			EventHandler value2 = TextC_MouseHover;
			TextBox textC = _TextC;
			if (textC != null)
			{
				textC.MouseHover -= value2;
			}
			_TextC = value;
			textC = _TextC;
			if (textC != null)
			{
				textC.MouseHover += value2;
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
			EventHandler value2 = Indi_Click;
			EventHandler value3 = Indi_MouseHover;
			PictureBox indi = _Indi;
			if (indi != null)
			{
				indi.Click -= value2;
				indi.MouseHover -= value3;
			}
			_Indi = value;
			indi = _Indi;
			if (indi != null)
			{
				indi.Click += value2;
				indi.MouseHover += value3;
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
			EventHandler value2 = IndiT_Click;
			EventHandler value3 = IndiT_MouseHover;
			PictureBox indiT = _IndiT;
			if (indiT != null)
			{
				indiT.Click -= value2;
				indiT.MouseHover -= value3;
			}
			_IndiT = value;
			indiT = _IndiT;
			if (indiT != null)
			{
				indiT.Click += value2;
				indiT.MouseHover += value3;
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
			EventHandler value2 = IndiK_Click;
			EventHandler value3 = IndiK_MouseHover;
			PictureBox indiK = _IndiK;
			if (indiK != null)
			{
				indiK.Click -= value2;
				indiK.MouseHover -= value3;
			}
			_IndiK = value;
			indiK = _IndiK;
			if (indiK != null)
			{
				indiK.Click += value2;
				indiK.MouseHover += value3;
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
			EventHandler value2 = VisB_Click;
			EventHandler value3 = VisB_MouseHover;
			PictureBox visB = _VisB;
			if (visB != null)
			{
				visB.Click -= value2;
				visB.MouseHover -= value3;
			}
			_VisB = value;
			visB = _VisB;
			if (visB != null)
			{
				visB.Click += value2;
				visB.MouseHover += value3;
			}
		}
	}

	[field: AccessedThroughProperty("ToolTip1")]
	internal virtual ToolTip ToolTip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	private virtual System.Timers.Timer Timer1
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
			System.Timers.Timer timer = _Timer1;
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		this.components = new System.ComponentModel.Container();
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormTimer));
		this.TextC = new System.Windows.Forms.TextBox();
		this.OfflineCount = new System.Windows.Forms.TextBox();
		this.Indi = new System.Windows.Forms.PictureBox();
		this.IndiT = new System.Windows.Forms.PictureBox();
		this.IndiK = new System.Windows.Forms.PictureBox();
		this.VisB = new System.Windows.Forms.PictureBox();
		this.ToolTip1 = new System.Windows.Forms.ToolTip(this.components);
		((System.ComponentModel.ISupportInitialize)this.Indi).BeginInit();
		((System.ComponentModel.ISupportInitialize)this.IndiT).BeginInit();
		((System.ComponentModel.ISupportInitialize)this.IndiK).BeginInit();
		((System.ComponentModel.ISupportInitialize)this.VisB).BeginInit();
		base.SuspendLayout();
		this.TextC.BorderStyle = System.Windows.Forms.BorderStyle.None;
		this.TextC.Enabled = false;
		this.TextC.Font = new System.Drawing.Font("Microsoft Sans Serif", 7.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TextC.Location = new System.Drawing.Point(2, 2);
		this.TextC.Name = "TextC";
		this.TextC.Size = new System.Drawing.Size(105, 15);
		this.TextC.TabIndex = 0;
		this.TextC.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.OfflineCount.BorderStyle = System.Windows.Forms.BorderStyle.None;
		this.OfflineCount.Enabled = false;
		this.OfflineCount.Font = new System.Drawing.Font("Microsoft Sans Serif", 7.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OfflineCount.Location = new System.Drawing.Point(2, 39);
		this.OfflineCount.Name = "OfflineCount";
		this.OfflineCount.Size = new System.Drawing.Size(105, 15);
		this.OfflineCount.TabIndex = 2;
		this.OfflineCount.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Indi.BorderStyle = System.Windows.Forms.BorderStyle.Fixed3D;
		this.Indi.Location = new System.Drawing.Point(2, 23);
		this.Indi.Name = "Indi";
		this.Indi.Size = new System.Drawing.Size(31, 10);
		this.Indi.TabIndex = 3;
		this.Indi.TabStop = false;
		this.IndiT.BorderStyle = System.Windows.Forms.BorderStyle.Fixed3D;
		this.IndiT.Location = new System.Drawing.Point(39, 23);
		this.IndiT.Name = "IndiT";
		this.IndiT.Size = new System.Drawing.Size(31, 10);
		this.IndiT.TabIndex = 4;
		this.IndiT.TabStop = false;
		this.IndiK.BorderStyle = System.Windows.Forms.BorderStyle.Fixed3D;
		this.IndiK.Location = new System.Drawing.Point(76, 23);
		this.IndiK.Name = "IndiK";
		this.IndiK.Size = new System.Drawing.Size(31, 10);
		this.IndiK.TabIndex = 5;
		this.IndiK.TabStop = false;
		this.VisB.BackColor = System.Drawing.Color.FromArgb(128, 255, 128);
		this.VisB.BorderStyle = System.Windows.Forms.BorderStyle.FixedSingle;
		this.VisB.Location = new System.Drawing.Point(113, 2);
		this.VisB.Name = "VisB";
		this.VisB.Size = new System.Drawing.Size(20, 57);
		this.VisB.SizeMode = System.Windows.Forms.PictureBoxSizeMode.Zoom;
		this.VisB.TabIndex = 6;
		this.VisB.TabStop = false;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(155, 59);
		base.ControlBox = false;
		base.Controls.Add(this.VisB);
		base.Controls.Add(this.IndiK);
		base.Controls.Add(this.IndiT);
		base.Controls.Add(this.Indi);
		base.Controls.Add(this.OfflineCount);
		base.Controls.Add(this.TextC);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.None;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormTimer";
		this.Text = "Счетчик";
		base.TopMost = true;
		((System.ComponentModel.ISupportInitialize)this.Indi).EndInit();
		((System.ComponentModel.ISupportInitialize)this.IndiT).EndInit();
		((System.ComponentModel.ISupportInitialize)this.IndiK).EndInit();
		((System.ComponentModel.ISupportInitialize)this.VisB).EndInit();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormTimer(string fn)
	{
		base.Load += FormTimer_Load;
		base.Closed += FormTimer_Closed;
		base.MouseClick += FormTimer_MouseClick;
		base.Closing += FormTimer_Closing;
		base.Resize += FormTimer_Resize;
		Timer1 = new System.Timers.Timer();
		tc = 0L;
		EndT = 3;
		Vis = true;
		AlertEnd = false;
		InitializeComponent();
		FNf = fn;
	}

	~FormTimer()
	{
		base.Finalize();
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
		base.Left = -1000;
		if (All.f.IntegerGetFn(All.A.FN, "ShowInTaskbar") != 0)
		{
			All.f.IntigerWriteFN(All.A.FN, "ShowInTaskbar", 1);
		}
		else
		{
			All.f.IntigerWriteFN(All.A.FN, "ShowInTaskbar", 0);
			base.ShowInTaskbar = false;
		}
		Indi.BackColor = Color.LightGreen;
		if (All.A.TypWork == 2020)
		{
			IndiK.BackColor = Color.Green;
		}
		else
		{
			IndiK.BackColor = Color.LightGreen;
		}
		IndiT.BackColor = Color.LightGreen;
		VisB.BackColor = Color.LightGreen;
		checked
		{
			base.Width = 2 * OfflineCount.Left + OfflineCount.Width;
			base.Height = 2 * OfflineCount.Left + OfflineCount.Top + OfflineCount.Height;
			if (StartControl().errCode > 0)
			{
				Timer1.Interval = 900.0;
				Timer1.Start();
				AlertEnd = true;
				Invoke((VB_0024AnonymousDelegate_0)([SpecialName] () =>
				{
					Close();
				}));
				return;
			}
			new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + FNf + "\\bl.ini").IntigerWriteFN(FNf, "Block", 0);
			Text = "ПРРО " + FNf;
			tc = 0L;
			if ((All.A.TypWork == 2020) & (All.A.PullY > 0))
			{
				base.Top = All.lC.Y(All.A.PullY);
			}
			else
			{
				nIndex = All.lC.YGet(FNf);
				base.Top = All.lC.Y(nIndex);
			}
			base.Left = -3;
			Vis = true;
			OperatorsAll operatorsAll = new OperatorsAll();
			Coding coding = new Coding();
			FullV = All.A.FullVersion;
			DS = new Dispatch(FNf, All.A.Connection, operatorsAll.get_Seller(2, 1), coding.DeCod(operatorsAll.get_Seller(3, 1)), All.A.AcskSettings, All.Lg.PathFile, All.A.FiscalMode, All.A.TIN);
			if (Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", TextCompare: false) == 0)
			{
				if (Environment.UserInteractive)
				{
					TextC.Text = FNf + " TS";
				}
				BackColor = Color.White;
				TextC.BackColor = Color.White;
				OfflineCount.BackColor = Color.White;
				TextC.ForeColor = Color.Black;
				OfflineCount.ForeColor = Color.Black;
			}
			else
			{
				if (Environment.UserInteractive)
				{
					TextC.Text = FNf;
				}
				TextC.ForeColor = Color.Black;
				OfflineCount.ForeColor = Color.Black;
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
			else if (Operators.CompareString(FNf, "7000000512", TextCompare: false) == 0)
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
			if (Environment.UserInteractive && Operators.CompareString(FNf, "7000000512", TextCompare: false) == 0)
			{
				OfflineCount.Text = "DEMO";
			}
			Application.DoEvents();
			Show();
			base.WindowState = FormWindowState.Normal;
			Application.DoEvents();
			if (All.A.TypWork == 2020)
			{
				DS.OreratorKeyDataEndIni();
				Timer1.Interval = 999.0;
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
		checked
		{
			if (AlertEnd)
			{
				Timer1.Stop();
				Invoke((VB_0024AnonymousDelegate_0)([SpecialName] () =>
				{
					Close();
				}));
			}
			else
			{
				if (All.Timer1Start)
				{
					return;
				}
				All.Timer1Start = true;
				if (Operators.CompareString(FNf, "7000000512", TextCompare: false) == 0)
				{
					Indi.BackColor = Color.LightGreen;
					VisB.BackColor = Indi.BackColor;
					if (Environment.UserInteractive)
					{
						OfflineCount.Text = "DEMO";
					}
					if (All.A.TypWork == 2020)
					{
						AlertEnd = true;
						Invoke((VB_0024AnonymousDelegate_0)([SpecialName] () =>
						{
							Close();
						}));
					}
					All.Timer1Start = false;
					return;
				}
				if (!FullV)
				{
					Indi.BackColor = Color.LightGreen;
					VisB.BackColor = Indi.BackColor;
					if (Environment.UserInteractive)
					{
						OfflineCount.Text = "FREE";
					}
					if (All.A.TypWork == 2020)
					{
						AlertEnd = true;
						Invoke((VB_0024AnonymousDelegate_0)([SpecialName] () =>
						{
							Close();
						}));
					}
					All.Timer1Start = false;
					return;
				}
				if (!All.SecondRunControlR())
				{
					AlertEnd = true;
					Invoke((VB_0024AnonymousDelegate_0)([SpecialName] () =>
					{
						Close();
					}));
					All.Timer1Start = false;
					return;
				}
				Timer1.Stop();
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
				else if (Operators.CompareString(FNf, "7000000512", TextCompare: false) == 0)
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
								Invoke((VB_0024AnonymousDelegate_0)([SpecialName] () =>
								{
									Close();
								}));
							}
						}
					}
					switch (num2)
					{
					case -3:
						if (All.A.TypWork == 2020)
						{
							Timer1.Interval = 27000.0;
							Indi.BackColor = Color.DarkRed;
						}
						else
						{
							Timer1.Interval = 1080000.0;
							Indi.BackColor = Color.DarkRed;
						}
						break;
					case -2:
						if (All.A.TypWork == 2020)
						{
							Timer1.Interval = 18000.0;
							Indi.BackColor = Color.Red;
						}
						else
						{
							Timer1.Interval = 270000.0;
							Indi.BackColor = Color.Red;
						}
						break;
					case -1:
						if (All.A.TypWork == 2020)
						{
							Timer1.Interval = 18000.0;
							Indi.BackColor = Color.Yellow;
						}
						else
						{
							Timer1.Interval = 270000.0;
							Indi.BackColor = Color.Yellow;
						}
						break;
					case 0:
						if (All.A.TypWork == 2020)
						{
							Timer1.Interval = 3000.0;
							Indi.BackColor = Color.LightGreen;
						}
						else
						{
							Timer1.Interval = 540000.0;
							Indi.BackColor = Color.LightGreen;
						}
						break;
					case 1:
						if (All.A.TypWork == 2020)
						{
							Timer1.Interval = 2000.0;
							Indi.BackColor = Color.Green;
						}
						else
						{
							Timer1.Interval = 9000.0;
							Indi.BackColor = Color.Green;
						}
						break;
					}
					if (num2 < 0 && (int)Math.Abs(DateAndTime.DateDiff(DateInterval.Second, now2, now)) > 21)
					{
						Timer1.Interval = 1620000.0;
						Indi.BackColor = Color.PaleVioletRed;
						if (All.A.TypWork == 2020)
						{
							Timer1.Interval = 9000.0;
							All.Lg.SaveTextToLog("Robot Server", "Ожидание ответа более 21 секунды");
							AlertEnd = true;
							Invoke((VB_0024AnonymousDelegate_0)([SpecialName] () =>
							{
								Close();
							}));
						}
					}
				}
				else
				{
					Indi.BackColor = Color.LightGreen;
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
							Invoke((VB_0024AnonymousDelegate_0)([SpecialName] () =>
							{
								Close();
							}));
						}
					}
					else
					{
						DS.OreratorKeyDataEndIni();
					}
				}
				VisB.BackColor = Indi.BackColor;
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
						IndiK.BackColor = Color.Green;
						break;
					case 2:
						IndiK.BackColor = Color.Yellow;
						break;
					case 1:
						IndiK.BackColor = Color.Pink;
						break;
					case 0:
						IndiK.BackColor = Color.Red;
						break;
					}
				}
				else
				{
					IndiK.BackColor = Color.LightGreen;
				}
				IndiOfflineCount(num);
				Application.DoEvents();
				Timer1.Start();
				All.Timer1Start = false;
			}
		}
	}

	private void IndiOfflineCount(int e)
	{
		if (e > 999)
		{
			IndiT.BackColor = Color.LightGreen;
		}
		else if (e > 499)
		{
			IndiT.BackColor = Color.GreenYellow;
		}
		else if (e > 199)
		{
			IndiT.BackColor = Color.YellowGreen;
		}
		else if (e > 99)
		{
			IndiT.BackColor = Color.LightPink;
		}
		else if (e > 49)
		{
			IndiT.BackColor = Color.LightCoral;
		}
		else if (e > 3)
		{
			IndiT.BackColor = Color.Coral;
		}
		else
		{
			IndiT.BackColor = Color.Red;
		}
	}

	private void FormTimer_Closed(object sender, EventArgs e)
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
		Dispose();
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
				base.Left = VisB.Left * -1;
				Vis = false;
				base.Width = VisB.Left + VisB.Width + OfflineCount.Left;
			}
			else
			{
				base.Left = -3;
				Vis = true;
				base.Width = 2 * OfflineCount.Left + OfflineCount.Width;
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
		ToolTip1.SetToolTip((Control)sender, "ПРРО " + FNf);
	}

	private void TextC_MouseHover(object sender, EventArgs e)
	{
		ToolTip1.SetToolTip((Control)sender, "ПРРО " + FNf);
	}

	private void Indi_MouseHover(object sender, EventArgs e)
	{
		ToolTip1.SetToolTip((Control)sender, "Индикатор офлайн режима");
	}

	private void IndiT_MouseHover(object sender, EventArgs e)
	{
		ToolTip1.SetToolTip((Control)sender, "Индикатор контроля времени работы");
	}

	private void IndiK_MouseHover(object sender, EventArgs e)
	{
		ToolTip1.SetToolTip((Control)sender, "Индикатор состояния ключей и сертификатов");
	}

	private void FormTimer_Closing(object sender, CancelEventArgs e)
	{
		if ((All.A.TypWork == 2020) | (All.A.TypWork == 2019))
		{
			e.Cancel = false;
		}
		else if (Interaction.MsgBox("УВАГА!   Закриття форми призведе до припинення відправки офлайн чеків. Закрити форму?", MsgBoxStyle.OkCancel | MsgBoxStyle.Question, "Контроль роботи офлайн!") == MsgBoxResult.Ok)
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
		base.WindowState = FormWindowState.Normal;
	}
}
